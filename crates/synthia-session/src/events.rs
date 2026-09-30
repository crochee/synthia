//! Typed session event log — the durable substrate Synthia uses
//! to reconstruct agent runs.
//!
//! Adopted from `deepseek-harness`'s `core/session/src/{types,
//! surface, repair}.ts`. Synthia's existing `SessionSink` trait
//! stores opaque `serde_json::Value` events; this module adds a
//! *strongly-typed* overlay on top of that wire without changing
//! the trait contract.
//!
//! See [`SessionEvent`] for the typed-event enum and
//! [`SurfaceOp`] for the append / replace discriminator.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::token_meter::UsageBuckets;

/// Borrowed view of a [`SurfaceOp::Replace`] payload.
#[derive(Debug, Clone, Copy)]
pub struct ReplaceRange<'a> {
    /// Inclusive start index of the replaced range.
    pub start: usize,
    /// Exclusive end index of the replaced range.
    pub end: usize,
    /// Cited source-event seqs.
    pub source_event_seqs: &'a [u64],
}

/// How a [`SessionEvent`] enters the model-visible surface.
///
/// Wire format:
///
/// - `"append"` (string) — plain append at the tail.
/// - `{"start": usize, "end": usize, "source_event_seqs":
///   [u64]}` (struct) — atomic replacement of `[start, end)`
///   in the folded surface. `source_event_seqs` cites the seqs
///   being shadowed so the fold can reject provenance violations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SurfaceOp {
    /// Plain append at the tail of the surface. Serialised as
    /// the snake_case string `"append"`.
    AppendString(String),
    /// Replacement of an inclusive range `[start, end)`
    /// (0-indexed) of the previously-folded surface.
    /// `source_event_seqs` cites the seqs being shadowed.
    Replace {
        /// Inclusive start index of the replaced range (in the
        /// folded surface).
        start: usize,
        /// Exclusive end index of the replaced range.
        end: usize,
        /// Seqs of the events being shadowed. The fold validator
        /// rejects replace ops whose `source_event_seqs` either
        /// (a) reference seqs outside the previous-log prefix or
        /// (b) are non-monotonic with the new event's seq.
        source_event_seqs: Vec<u64>,
    },
}

impl SurfaceOp {
    /// Wire name for the append form.
    pub const APPEND: &'static str = "append";

    /// Build the append variant.
    #[must_use]
    pub fn append() -> Self {
        Self::AppendString(Self::APPEND.to_string())
    }

    /// True when the op replaces a previously-folded range.
    #[must_use]
    pub const fn is_replace(&self) -> bool {
        matches!(self, Self::Replace { .. })
    }

    /// True when the op appends to the tail of the surface.
    #[must_use]
    pub const fn is_append(&self) -> bool {
        matches!(self, Self::AppendString(_))
    }

    /// Replace variant accessor (`None` for append).
    #[must_use]
    pub fn as_replace(&self) -> Option<ReplaceRange<'_>> {
        match self {
            Self::Replace {
                start,
                end,
                source_event_seqs,
            } => Some(ReplaceRange {
                start: *start,
                end: *end,
                source_event_seqs,
            }),
            Self::AppendString(_) => None,
        }
    }
}

impl Default for SurfaceOp {
    fn default() -> Self {
        Self::append()
    }
}

/// One entry in the durable session event log.
///
/// `seq` is the monotonic ordinal assigned by the sink at append
/// time. Callers MUST NOT pick seq manually — the sink stamps it
/// on `read()` (returning events in the canonical order) and on
/// `fold_surface` (which validates seq continuity).
///
/// The `data` payload on message-emitting variants is a JSON
/// `Value` so the wire shape stays decoupled from
/// `synthia_provider::Message`'s exact serialization (which may
/// grow new content-part variants over time).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionEvent {
    /// User-role message — surface-eligible.
    #[serde(rename = "user_message")]
    UserMessage {
        /// Sequence ordinal stamped by the sink.
        seq: u64,
        /// ISO-8601 timestamp.
        ts: String,
        /// Message payload (matches `synthia_provider::Message`).
        data: Value,
        /// How this event joins the surface.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        surface_op: Option<SurfaceOp>,
    },
    /// Assistant-role message — surface-eligible.
    #[serde(rename = "assistant_message")]
    AssistantMessage {
        /// Sequence ordinal.
        seq: u64,
        /// ISO-8601 timestamp.
        ts: String,
        /// Message payload.
        data: Value,
        /// How this event joins the surface.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        surface_op: Option<SurfaceOp>,
    },
    /// One streamed provider delta, preserved verbatim for
    /// replay fidelity — log-only.
    ///
    /// The agent loop emits one of these per wire delta
    /// alongside the eventually-complete `AssistantMessage`.
    /// Rebuild the turn's text with
    /// [`crate::surface::assemble_chunks`].
    #[serde(rename = "assistant_chunk")]
    AssistantChunk {
        /// Sequence ordinal.
        seq: u64,
        /// ISO-8601 timestamp.
        ts: String,
        /// Monotonic ordinal of this chunk within its turn
        /// (0-based, assigned by the emitter).
        chunk_seq: u64,
        /// Provider stop reason, when this chunk is terminal.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        finish_reason: Option<String>,
        /// Chunk payload (`{"delta": "<text>"}`).
        data: Value,
    },
    /// Tool-result message — surface-eligible.
    #[serde(rename = "tool_result")]
    ToolResult {
        /// Sequence ordinal.
        seq: u64,
        /// ISO-8601 timestamp.
        ts: String,
        /// Message payload.
        data: Value,
        /// How this event joins the surface.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        surface_op: Option<SurfaceOp>,
    },
    /// Summary replacing a span of older tool-results.
    ///
    /// Used by the `SummarizingContextManager` to swap a tail of
    /// noisy tool results for one compact assistant message while
    /// keeping the durable log lossless.
    #[serde(rename = "compaction")]
    Compaction {
        /// Sequence ordinal.
        seq: u64,
        /// ISO-8601 timestamp.
        ts: String,
        /// Summary text (assistant message body).
        data: Value,
        /// Surface op (replace with cited source seqs).
        surface_op: SurfaceOp,
    },
    /// Compaction began — log-only.
    ///
    /// An unmatched `CompactionStart` (no matching
    /// `CompactionEnd`) is the crash marker the loader detects;
    /// see [`crate::repair::orphaned_compactions`].
    #[serde(rename = "compaction_start")]
    CompactionStart {
        /// Sequence ordinal.
        seq: u64,
        /// ISO-8601 timestamp.
        ts: String,
        /// Opaque id correlating Start/Summary/End.
        token: String,
        /// Compaction metadata (reason, budget, ...).
        data: Value,
    },
    /// Compaction produced a summary — log-only.
    #[serde(rename = "compaction_summary")]
    CompactionSummary {
        /// Sequence ordinal.
        seq: u64,
        /// ISO-8601 timestamp.
        ts: String,
        /// `token` of the matching `CompactionStart`.
        token: String,
        /// Summary payload (`{"summary": "<text>"}`).
        data: Value,
    },
    /// Compaction finished (success or failure) — log-only.
    #[serde(rename = "compaction_end")]
    CompactionEnd {
        /// Sequence ordinal.
        seq: u64,
        /// ISO-8601 timestamp.
        ts: String,
        /// `token` of the matching `CompactionStart`.
        token: String,
        /// `"committed"` | `"failed"` | `"interrupted"` (see
        /// [`CompactionOutcome`]).
        outcome: String,
        /// Terminal-state payload.
        data: Value,
    },
    /// A tool call the model emitted — log-only, no surface.
    #[serde(rename = "tool_call")]
    ToolCall {
        /// Sequence ordinal.
        seq: u64,
        /// ISO-8601 timestamp.
        ts: String,
        /// Call id, tool name, raw arguments JSON.
        data: Value,
    },
    /// Step boundary (`start` | `end`) — log-only.
    #[serde(rename = "step")]
    Step {
        /// Sequence ordinal.
        seq: u64,
        /// ISO-8601 timestamp.
        ts: String,
        /// `start` or `end`, plus `turn` and `step` indices.
        data: Value,
    },
    /// Turn boundary (`start` | `end`) — log-only.
    #[serde(rename = "turn")]
    Turn {
        /// Sequence ordinal.
        seq: u64,
        /// ISO-8601 timestamp.
        ts: String,
        /// `start` or `end`, plus `turn` index and end reason.
        data: Value,
    },
    /// Iteration boundary (`start` | `end`) — log-only.
    #[serde(rename = "iteration")]
    Iteration {
        /// Sequence ordinal.
        seq: u64,
        /// ISO-8601 timestamp.
        ts: String,
        /// `start` or `end`, plus iteration index.
        data: Value,
    },
    /// Lifecycle warning — log-only.
    #[serde(rename = "warning")]
    Warning {
        /// Sequence ordinal.
        seq: u64,
        /// ISO-8601 timestamp.
        ts: String,
        /// Warning kind + message + iteration.
        data: Value,
    },
    /// Steering guard decision — log-only.
    #[serde(rename = "steering_guard")]
    SteeringGuard {
        /// Sequence ordinal.
        seq: u64,
        /// ISO-8601 timestamp.
        ts: String,
        /// Guard name + action + reason.
        data: Value,
    },
    /// Steering hint injection — log-only.
    #[serde(rename = "steering_hint")]
    SteeringHint {
        /// Sequence ordinal.
        seq: u64,
        /// ISO-8601 timestamp.
        ts: String,
        /// Hint name + content + injection point.
        data: Value,
    },
    /// Hook veto — log-only.
    #[serde(rename = "hook_block")]
    HookBlock {
        /// Sequence ordinal.
        seq: u64,
        /// ISO-8601 timestamp.
        ts: String,
        /// Hook name + reason.
        data: Value,
    },
    /// Sub-agent delegation entry — log-only.
    #[serde(rename = "subagent_enter")]
    SubagentEnter {
        /// Sequence ordinal.
        seq: u64,
        /// ISO-8601 timestamp.
        ts: String,
        /// Child session id + parent + depth.
        data: Value,
    },
    /// Sub-agent delegation exit — log-only.
    #[serde(rename = "subagent_exit")]
    SubagentExit {
        /// Sequence ordinal.
        seq: u64,
        /// ISO-8601 timestamp.
        ts: String,
        /// Child session id + parent + end reason.
        data: Value,
    },
    /// Per-call request config snapshot — log-only.
    #[serde(rename = "request_header")]
    RequestHeader {
        /// Sequence ordinal.
        seq: u64,
        /// ISO-8601 timestamp.
        ts: String,
        /// Epoch + provider + model + tool/system hashes.
        data: Value,
    },
    /// Per-call usage attribution — log-only.
    ///
    /// R30: the provider report is carried in two shapes. The
    /// typed `usage` field is the current wire form (serde
    /// default, so pre-R30 rows parse with `None`); the legacy
    /// `data` payload (`prompt_tokens` / `completion_tokens` /
    /// `cache_read_tokens` / `cache_write_tokens`) is still
    /// stamped so older readers keep working. Read either shape
    /// through [`SessionEvent::provider_usage`].
    #[serde(rename = "usage")]
    Usage {
        /// Sequence ordinal.
        seq: u64,
        /// ISO-8601 timestamp.
        ts: String,
        /// Epoch + token breakdown (legacy envelope).
        data: Value,
        /// Typed disjoint provider usage report (R30), absent on
        /// older rows.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        usage: Option<UsageBuckets>,
    },
    /// Session sandbox-mode override — log-only.
    ///
    /// A policy switch (a UI control, a delegation seed, a test
    /// fixture) is recorded as one of these on the session it
    /// applies to. The log IS the store: the override survives a
    /// restart by replay, and two sessions can never see each
    /// other's state (dsh `sandbox/mode` parity). The effective
    /// policy is the last such event folded against the caller's
    /// grants and the deployment default — see
    /// `synthia_tool_shell::sandbox::effective_policy`.
    #[serde(rename = "sandbox_mode")]
    SandboxMode {
        /// Sequence ordinal.
        seq: u64,
        /// ISO-8601 timestamp.
        ts: String,
        /// Execution-policy wire name (`"read-only"` /
        /// `"workspace-write"` / `"danger-full-access"`). Carried
        /// as a string rather than an enum: a log written by a
        /// newer build must round-trip here, and a mode this
        /// build does not know is ignored by the policy fold.
        mode: String,
    },
    /// Session shutdown lifecycle — log-only.
    ///
    /// Emitted by the server's `SessionController::close` after
    /// all child sessions have been signalled, so the log ends
    /// with an explicit, replayable shutdown marker.
    #[serde(rename = "session_shutdown")]
    LifecycleShutdown {
        /// Sequence ordinal.
        seq: u64,
        /// ISO-8601 timestamp.
        ts: String,
        /// Shutdown metadata (reason, child count, ...).
        data: Value,
    },
}

impl SessionEvent {
    /// Sequence ordinal stamped by the sink.
    #[must_use]
    pub const fn seq(&self) -> u64 {
        match self {
            Self::UserMessage { seq, .. }
            | Self::AssistantMessage { seq, .. }
            | Self::AssistantChunk { seq, .. }
            | Self::ToolResult { seq, .. }
            | Self::Compaction { seq, .. }
            | Self::CompactionStart { seq, .. }
            | Self::CompactionSummary { seq, .. }
            | Self::CompactionEnd { seq, .. }
            | Self::ToolCall { seq, .. }
            | Self::Step { seq, .. }
            | Self::Turn { seq, .. }
            | Self::Iteration { seq, .. }
            | Self::Warning { seq, .. }
            | Self::SteeringGuard { seq, .. }
            | Self::SteeringHint { seq, .. }
            | Self::HookBlock { seq, .. }
            | Self::SubagentEnter { seq, .. }
            | Self::SubagentExit { seq, .. }
            | Self::RequestHeader { seq, .. }
            | Self::LifecycleShutdown { seq, .. }
            | Self::SandboxMode { seq, .. }
            | Self::Usage { seq, .. } => *seq,
        }
    }

    /// True when the event can carry a `surface_op` and enter the
    /// replay view.
    #[must_use]
    pub const fn is_surface_eligible(&self) -> bool {
        matches!(
            self,
            Self::UserMessage { .. }
                | Self::AssistantMessage { .. }
                | Self::ToolResult { .. }
                | Self::Compaction { .. }
        )
    }

    /// `surface_op` value if the event carries one.
    #[must_use]
    pub fn surface_op(&self) -> Option<&SurfaceOp> {
        match self {
            Self::UserMessage { surface_op, .. }
            | Self::AssistantMessage { surface_op, .. }
            | Self::ToolResult { surface_op, .. } => surface_op.as_ref(),
            Self::Compaction { surface_op, .. } => Some(surface_op),
            _ => None,
        }
    }

    /// Provider usage reported by this event, if any.
    ///
    /// [`SessionEvent::Usage`] rows carry the report either in
    /// the typed R30 `usage` field or — on pre-R30 rows — in the
    /// legacy `data` payload (`prompt_tokens` /
    /// `completion_tokens` / `cache_read_tokens` /
    /// `cache_write_tokens`). Both shapes resolve to the same
    /// disjoint [`UsageBuckets`]. Non-usage events, and usage
    /// rows without a decodable report, return `None`.
    #[must_use]
    pub fn provider_usage(&self) -> Option<UsageBuckets> {
        let Self::Usage { usage, data, .. } = self else {
            return None;
        };
        match usage {
            Some(buckets) => Some(*buckets),
            None => parse_legacy_usage(data),
        }
    }

    /// Policy wire name carried by a
    /// [`SessionEvent::SandboxMode`] row, if this is one.
    ///
    /// Returns the raw string, not a parsed policy: the wire layer
    /// does not know the policy enum, and an unknown mode must
    /// survive a read-back for a newer build to interpret.
    #[must_use]
    pub fn sandbox_mode(&self) -> Option<&str> {
        match self {
            Self::SandboxMode { mode, .. } => Some(mode),
            _ => None,
        }
    }

    /// Parse a [`SessionEvent`] from the opaque `serde_json::Value`
    /// produced by `SessionSink::read`. Returns `None` for legacy
    /// shapes that pre-date the typed layer (`{"role": ...}`,
    /// `{"type": "UserInput"}`, etc.) — callers handle those via
    /// the existing `events_to_messages` projection.
    #[must_use]
    pub fn from_value(v: &Value) -> Option<Self> {
        let obj = v.as_object()?;
        let tag = obj.get("type")?.as_str()?;
        let known = matches!(
            tag,
            "user_message"
                | "assistant_message"
                | "assistant_chunk"
                | "tool_result"
                | "compaction"
                | "compaction_start"
                | "compaction_summary"
                | "compaction_end"
                | "tool_call"
                | "step"
                | "turn"
                | "iteration"
                | "warning"
                | "steering_guard"
                | "steering_hint"
                | "hook_block"
                | "subagent_enter"
                | "subagent_exit"
                | "request_header"
                | "usage"
                | "sandbox_mode"
                | "session_shutdown"
        );
        if !known {
            return None;
        }
        serde_json::from_value(v.clone()).ok()
    }

    /// Stable wire-tag string (snake_case).
    #[must_use]
    pub const fn type_tag(&self) -> &'static str {
        match self {
            Self::UserMessage { .. } => "user_message",
            Self::AssistantMessage { .. } => "assistant_message",
            Self::AssistantChunk { .. } => "assistant_chunk",
            Self::ToolResult { .. } => "tool_result",
            Self::Compaction { .. } => "compaction",
            Self::CompactionStart { .. } => "compaction_start",
            Self::CompactionSummary { .. } => "compaction_summary",
            Self::CompactionEnd { .. } => "compaction_end",
            Self::ToolCall { .. } => "tool_call",
            Self::Step { .. } => "step",
            Self::Turn { .. } => "turn",
            Self::Iteration { .. } => "iteration",
            Self::Warning { .. } => "warning",
            Self::SteeringGuard { .. } => "steering_guard",
            Self::SteeringHint { .. } => "steering_hint",
            Self::HookBlock { .. } => "hook_block",
            Self::SubagentEnter { .. } => "subagent_enter",
            Self::SubagentExit { .. } => "subagent_exit",
            Self::RequestHeader { .. } => "request_header",
            Self::Usage { .. } => "usage",
            Self::SandboxMode { .. } => "sandbox_mode",
            Self::LifecycleShutdown { .. } => "session_shutdown",
        }
    }
}

/// Decode the pre-R30 usage payload shape (`data` carries the
/// buckets as flat keys). Returns `None` when the required
/// `prompt_tokens` key is absent — such a row is not a sample.
fn parse_legacy_usage(data: &Value) -> Option<UsageBuckets> {
    let input_tokens = data.get("prompt_tokens")?.as_u64()?;
    let output_tokens = data
        .get("completion_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let cache_read_tokens =
        data.get("cache_read_tokens").and_then(Value::as_u64);
    let cache_write_tokens =
        data.get("cache_write_tokens").and_then(Value::as_u64);
    Some(UsageBuckets {
        input_tokens,
        output_tokens,
        cache_read_tokens,
        cache_write_tokens,
    })
}

/// Opaque correlation id for one compaction lifecycle.
///
/// Threaded through [`SessionEvent::CompactionStart`],
/// [`SessionEvent::CompactionSummary`], and
/// [`SessionEvent::CompactionEnd`] so the three can be matched
/// without the emitter inventing an id scheme. Serialises
/// transparently as the bare string.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SurfaceToken(String);

impl SurfaceToken {
    /// Mint a fresh ULID-backed token.
    #[must_use]
    pub fn new() -> Self {
        Self(ulid::Ulid::generate().to_string())
    }

    /// Wrap a token string the caller already minted — restore
    /// from a durable log, or pin a token in a test.
    #[must_use]
    pub fn from_string(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Borrow the underlying string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for SurfaceToken {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SurfaceToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Terminal state of one compaction lifecycle.
///
/// Serialises as the `outcome` string carried by
/// [`SessionEvent::CompactionEnd`]; [`Self::Interrupted`] is the
/// state a loader synthesises for an orphaned
/// [`SessionEvent::CompactionStart`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompactionOutcome {
    /// The summary was committed to the surface.
    Committed,
    /// The compaction errored; the surface is unchanged.
    Failed,
    /// The process died mid-compaction.
    Interrupted,
}

impl CompactionOutcome {
    /// Stable wire tag (snake_case), matching the `outcome`
    /// field on [`SessionEvent::CompactionEnd`].
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Committed => "committed",
            Self::Failed => "failed",
            Self::Interrupted => "interrupted",
        }
    }
}

/// Every typed event variant synthesia currently understands. Mirrors
/// dsh `KNOWN_SESSION_EVENT_TYPES` (the persistence layer refuses
/// to interpret a log containing a type outside this set unless
/// the row carries the legacy envelope marker).
pub const KNOWN_SESSION_EVENT_TYPES: &[&str] = &[
    "user_message",
    "assistant_message",
    "assistant_chunk",
    "tool_result",
    "compaction",
    "compaction_start",
    "compaction_summary",
    "compaction_end",
    "tool_call",
    "step",
    "turn",
    "iteration",
    "warning",
    "steering_guard",
    "steering_hint",
    "hook_block",
    "subagent_enter",
    "subagent_exit",
    "request_header",
    "usage",
    "sandbox_mode",
    "session_shutdown",
];

/// Build an empty event skeleton for one of the typed `type_tag`
/// strings. Returns `None` for any string outside
/// [`KNOWN_SESSION_EVENT_TYPES`]. The skeleton has zeroed `seq`,
/// an empty `ts`, a `Value::Null` `data` (an empty `mode` on the
/// `data`-less `sandbox_mode` variant), and (for
/// surface-eligible variants) the `append` `surface_op` so the
/// caller can recognise the discriminant before populating the
/// payload.
#[must_use]
pub fn empty_event_of(tag: &str) -> Option<SessionEvent> {
    let event = match tag {
        "user_message" => SessionEvent::UserMessage {
            seq: 0,
            ts: String::new(),
            data: Value::Null,
            surface_op: Some(SurfaceOp::AppendString("append".to_string())),
        },
        "assistant_message" => SessionEvent::AssistantMessage {
            seq: 0,
            ts: String::new(),
            data: Value::Null,
            surface_op: Some(SurfaceOp::AppendString("append".to_string())),
        },
        "assistant_chunk" => SessionEvent::AssistantChunk {
            seq: 0,
            ts: String::new(),
            chunk_seq: 0,
            finish_reason: None,
            data: Value::Null,
        },
        "tool_result" => SessionEvent::ToolResult {
            seq: 0,
            ts: String::new(),
            data: Value::Null,
            surface_op: Some(SurfaceOp::AppendString("append".to_string())),
        },
        "compaction" => SessionEvent::Compaction {
            seq: 0,
            ts: String::new(),
            data: Value::Null,
            surface_op: SurfaceOp::AppendString("append".to_string()),
        },
        "compaction_start" => SessionEvent::CompactionStart {
            seq: 0,
            ts: String::new(),
            token: String::new(),
            data: Value::Null,
        },
        "compaction_summary" => SessionEvent::CompactionSummary {
            seq: 0,
            ts: String::new(),
            token: String::new(),
            data: Value::Null,
        },
        "compaction_end" => SessionEvent::CompactionEnd {
            seq: 0,
            ts: String::new(),
            token: String::new(),
            outcome: String::new(),
            data: Value::Null,
        },
        "tool_call" => SessionEvent::ToolCall {
            seq: 0,
            ts: String::new(),
            data: Value::Null,
        },
        "step" => SessionEvent::Step {
            seq: 0,
            ts: String::new(),
            data: Value::Null,
        },
        "turn" => SessionEvent::Turn {
            seq: 0,
            ts: String::new(),
            data: Value::Null,
        },
        "iteration" => SessionEvent::Iteration {
            seq: 0,
            ts: String::new(),
            data: Value::Null,
        },
        "warning" => SessionEvent::Warning {
            seq: 0,
            ts: String::new(),
            data: Value::Null,
        },
        "steering_guard" => SessionEvent::SteeringGuard {
            seq: 0,
            ts: String::new(),
            data: Value::Null,
        },
        "steering_hint" => SessionEvent::SteeringHint {
            seq: 0,
            ts: String::new(),
            data: Value::Null,
        },
        "hook_block" => SessionEvent::HookBlock {
            seq: 0,
            ts: String::new(),
            data: Value::Null,
        },
        "subagent_enter" => SessionEvent::SubagentEnter {
            seq: 0,
            ts: String::new(),
            data: Value::Null,
        },
        "subagent_exit" => SessionEvent::SubagentExit {
            seq: 0,
            ts: String::new(),
            data: Value::Null,
        },
        "request_header" => SessionEvent::RequestHeader {
            seq: 0,
            ts: String::new(),
            data: Value::Null,
        },
        "usage" => SessionEvent::Usage {
            seq: 0,
            ts: String::new(),
            data: Value::Null,
            usage: None,
        },
        "sandbox_mode" => SessionEvent::SandboxMode {
            seq: 0,
            ts: String::new(),
            mode: String::new(),
        },
        "session_shutdown" => SessionEvent::LifecycleShutdown {
            seq: 0,
            ts: String::new(),
            data: Value::Null,
        },
        _ => return None,
    };
    Some(event)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// Non-`data` fields a tag requires beyond `seq`/`ts`.
    fn extra_fields(tag: &str) -> Value {
        match tag {
            "assistant_chunk" => json!({"chunk_seq": 7}),
            "compaction_start" | "compaction_summary" => {
                json!({"token": "t1"})
            }
            "compaction_end" => json!({"token": "t1", "outcome": "committed"}),
            "sandbox_mode" => json!({"mode": "workspace-write"}),
            _ => json!({}),
        }
    }

    fn event_json(tag: &str, seq: u64, with_surface_op: bool) -> Value {
        let mut v = json!({
            "seq": seq,
            "ts": "2026-09-10T00:00:00Z",
            "type": tag,
            "data": {"text": "hello"},
        });
        let obj = v.as_object_mut().unwrap();
        for (key, value) in extra_fields(tag).as_object().unwrap() {
            obj.insert(key.clone(), value.clone());
        }
        if with_surface_op {
            obj.insert("surface_op".to_string(), json!("append"));
        }
        v
    }

    #[test]
    fn parses_user_message_with_surface_op() {
        let v = event_json("user_message", 0, true);
        let ev = SessionEvent::from_value(&v).expect("parses");
        assert!(ev.is_surface_eligible());
        assert_eq!(ev.seq(), 0);
        assert!(ev.surface_op().unwrap().is_append());
        assert_eq!(ev.type_tag(), "user_message");
    }

    #[test]
    fn parses_assistant_message_with_replace_op() {
        let v = json!({
            "seq": 7,
            "ts": "2026-09-10T00:00:00Z",
            "type": "assistant_message",
            "data": {"text": "summary"},
            "surface_op": {
                "start": 1,
                "end": 5,
                "source_event_seqs": [3, 4],
            }
        });
        let ev = SessionEvent::from_value(&v).expect("parses");
        let op = ev.surface_op().expect("has surface_op");
        assert!(op.is_replace());
        let r = op.as_replace().unwrap();
        assert_eq!(r.start, 1);
        assert_eq!(r.end, 5);
        assert_eq!(r.source_event_seqs, &[3, 4]);
    }

    #[test]
    fn parses_compaction_with_surface_op() {
        let v = json!({
            "seq": 12,
            "ts": "2026-09-10T00:00:00Z",
            "type": "compaction",
            "data": {"text": "summary"},
            "surface_op": {
                "start": 0,
                "end": 4,
                "source_event_seqs": [1, 2, 3, 4]
            }
        });
        let ev = SessionEvent::from_value(&v).expect("parses");
        assert!(ev.is_surface_eligible());
        assert!(ev.surface_op().unwrap().is_replace());
    }

    #[test]
    fn log_only_events_carry_no_surface_op() {
        let log_only = [
            "assistant_chunk",
            "compaction_start",
            "compaction_summary",
            "compaction_end",
            "tool_call",
            "step",
            "turn",
            "iteration",
            "warning",
            "steering_guard",
            "steering_hint",
            "hook_block",
            "subagent_enter",
            "subagent_exit",
            "request_header",
            "usage",
            "sandbox_mode",
            "session_shutdown",
        ];
        for tag in log_only {
            let v = event_json(tag, 99, false);
            let ev = SessionEvent::from_value(&v)
                .unwrap_or_else(|| panic!("{tag} should parse"));
            assert!(!ev.is_surface_eligible(), "{tag} should not be surface");
            assert!(ev.surface_op().is_none(), "{tag} should carry no op");
        }
    }

    #[test]
    fn rejects_unknown_legacy_shape() {
        let legacy = json!({"role": "user", "text": "hi"});
        assert!(SessionEvent::from_value(&legacy).is_none());
        let legacy2 = json!({"type": "UserInput", "data": {"text": "go"}});
        assert!(SessionEvent::from_value(&legacy2).is_none());
    }

    #[test]
    fn round_trips_through_json() {
        let v = json!({
            "seq": 3,
            "ts": "2026-09-10T00:00:00Z",
            "type": "tool_result",
            "data": {"call_id": "c1", "content": "ok"},
            "surface_op": "append"
        });
        let ev = SessionEvent::from_value(&v).unwrap();
        let s = serde_json::to_string(&ev).unwrap();
        let back = SessionEvent::from_value(
            &serde_json::from_str::<Value>(&s).unwrap(),
        )
        .expect("round-trip");
        assert_eq!(back.seq(), 3);
        assert!(back.is_surface_eligible());
        assert!(back.surface_op().unwrap().is_append());
    }

    #[test]
    fn surface_op_default_is_append() {
        assert!(SurfaceOp::default().is_append());
    }

    #[test]
    fn seq_accessor_returns_payload_value() {
        let v = event_json("step", 42, false);
        let ev = SessionEvent::from_value(&v).unwrap();
        assert_eq!(ev.seq(), 42);
    }

    #[test]
    fn append_helper_round_trips_through_string_form() {
        let op = SurfaceOp::append();
        let s = serde_json::to_string(&op).unwrap();
        assert_eq!(s, "\"append\"");
        let parsed: SurfaceOp = serde_json::from_str(&s).unwrap();
        assert!(parsed.is_append());
    }

    #[test]
    fn compaction_event_requires_surface_op() {
        let v = json!({
            "seq": 1,
            "ts": "2026-09-10T00:00:00Z",
            "type": "compaction",
            "data": {"text": "summary"},
        });
        // Compaction always carries a surface op; missing one
        // MUST fail deserialization.
        assert!(SessionEvent::from_value(&v).is_none());
    }

    #[test]
    fn known_session_event_types_matches_type_tag_arm_count() {
        // Every wire tag in `KNOWN_SESSION_EVENT_TYPES` must round-trip
        // through `empty_event_of` -> `type_tag`. The round trip must
        // be stable across the wire alphabet.
        for tag in KNOWN_SESSION_EVENT_TYPES {
            let event = empty_event_of(tag).expect(tag);
            assert_eq!(event.type_tag(), *tag);
        }
    }

    #[test]
    fn known_session_event_types_round_trips_via_serde() {
        // Each skeleton must serialise + deserialise without losing
        // its type discriminant; downstream consumers may persist
        // the skeleton and expect the shape to round-trip.
        for tag in KNOWN_SESSION_EVENT_TYPES {
            let event = empty_event_of(tag).expect(tag);
            let json = serde_json::to_value(&event).unwrap();
            assert_eq!(json["type"].as_str(), Some(*tag));
            let restored: SessionEvent = serde_json::from_value(json).unwrap();
            assert_eq!(restored.type_tag(), *tag);
        }
    }

    #[test]
    fn empty_event_of_returns_none_for_unknown_tag() {
        assert!(empty_event_of("not_a_real_event").is_none());
        assert!(empty_event_of("").is_none());
    }

    #[test]
    fn known_session_event_types_has_one_entry_per_variant() {
        // The const is the public contract; every variant of
        // `SessionEvent` must appear exactly once. This catches
        // the case where a new variant is added but the const
        // is forgotten.
        let mut seen = std::collections::HashSet::new();
        for tag in KNOWN_SESSION_EVENT_TYPES {
            assert!(
                seen.insert(*tag),
                "duplicate tag in KNOWN_SESSION_EVENT_TYPES: {tag}"
            );
        }
        assert_eq!(
            seen.len(),
            22,
            "expected 22 typed event variants; add the new one to KNOWN_SESSION_EVENT_TYPES"
        );
    }

    #[test]
    fn assistant_chunk_round_trips_and_omits_finish_reason_when_none() {
        let v = json!({
            "seq": 4,
            "ts": "2026-09-10T00:00:00Z",
            "type": "assistant_chunk",
            "chunk_seq": 2,
            "data": {"delta": "he"},
        });
        let ev = SessionEvent::from_value(&v).expect("parses");
        assert_eq!(ev.seq(), 4);
        assert_eq!(ev.type_tag(), "assistant_chunk");
        assert!(!ev.is_surface_eligible());
        let SessionEvent::AssistantChunk {
            chunk_seq,
            finish_reason,
            ..
        } = &ev
        else {
            panic!("wrong variant");
        };
        assert_eq!(*chunk_seq, 2);
        assert!(finish_reason.is_none());

        // `None` is omitted from the wire rather than written as
        // `null`, so a non-terminal chunk stays shape-stable.
        let s = serde_json::to_string(&ev).unwrap();
        assert!(!s.contains("finish_reason"), "got {s}");
        let back: SessionEvent = serde_json::from_str(&s).unwrap();
        assert_eq!(back.type_tag(), "assistant_chunk");

        // A terminal chunk carries the provider stop reason.
        let terminal = json!({
            "seq": 5,
            "ts": "2026-09-10T00:00:00Z",
            "type": "assistant_chunk",
            "chunk_seq": 3,
            "finish_reason": "end_turn",
            "data": {"delta": ""},
        });
        let ev = SessionEvent::from_value(&terminal).expect("parses");
        let SessionEvent::AssistantChunk { finish_reason, .. } = &ev else {
            panic!("wrong variant");
        };
        assert_eq!(finish_reason.as_deref(), Some("end_turn"));
        let s = serde_json::to_string(&ev).unwrap();
        assert!(s.contains("\"finish_reason\":\"end_turn\""), "got {s}");
    }

    #[test]
    fn lifecycle_shutdown_round_trips() {
        let v = json!({
            "seq": 11,
            "ts": "2026-09-10T00:00:00Z",
            "type": "session_shutdown",
            "data": {"reason": "controller_close", "children": 2},
        });
        let ev = SessionEvent::from_value(&v).expect("parses");
        assert_eq!(ev.type_tag(), "session_shutdown");
        assert_eq!(ev.seq(), 11);
        assert!(!ev.is_surface_eligible());
        assert!(ev.surface_op().is_none());

        let s = serde_json::to_string(&ev).unwrap();
        let restored = SessionEvent::from_value(
            &serde_json::from_str::<Value>(&s).unwrap(),
        )
        .expect("round-trip");
        assert_eq!(restored.type_tag(), "session_shutdown");
        assert_eq!(restored.seq(), 11);
    }

    /// R31: the sandbox-mode override rides the log as one
    /// log-only row. A row written by a newer build — a mode this
    /// build does not know — must still parse, because the policy
    /// fold ignores what it cannot interpret rather than failing
    /// the log.
    #[test]
    fn sandbox_mode_row_parses_and_preserves_unknown_modes() {
        let ev =
            SessionEvent::from_value(&event_json("sandbox_mode", 9, false))
                .expect("parses");
        assert_eq!(ev.type_tag(), "sandbox_mode");
        assert_eq!(ev.seq(), 9);
        assert_eq!(ev.sandbox_mode(), Some("workspace-write"));
        assert!(!ev.is_surface_eligible());
        assert!(ev.surface_op().is_none());

        let unknown = json!({
            "seq": 10,
            "ts": "2026-09-12T00:00:00Z",
            "type": "sandbox_mode",
            "mode": "future-mode",
        });
        let ev = SessionEvent::from_value(&unknown).expect("parses");
        assert_eq!(ev.sandbox_mode(), Some("future-mode"));

        // A row that predates the variant never misreads as one.
        let legacy = SessionEvent::from_value(&event_json("warning", 1, false))
            .expect("parses");
        assert!(legacy.sandbox_mode().is_none());
    }

    #[test]
    fn compaction_outcome_serialises_snake_case() {
        for (outcome, wire) in [
            (CompactionOutcome::Committed, "committed"),
            (CompactionOutcome::Failed, "failed"),
            (CompactionOutcome::Interrupted, "interrupted"),
        ] {
            assert_eq!(
                serde_json::to_string(&outcome).unwrap(),
                format!("\"{wire}\"")
            );
            let back: CompactionOutcome =
                serde_json::from_str(&format!("\"{wire}\"")).unwrap();
            assert_eq!(back, outcome);
            assert_eq!(outcome.as_str(), wire);
        }
        assert!(
            serde_json::from_str::<CompactionOutcome>("\"bogus\"").is_err()
        );
    }

    #[test]
    fn surface_token_mints_ulids_and_serialises_transparently() {
        let minted = SurfaceToken::new();
        assert_eq!(minted.as_str().len(), 26, "ULID text is 26 chars");
        assert_eq!(minted.to_string(), minted.as_str());
        assert_eq!(
            serde_json::to_string(&minted).unwrap(),
            format!("\"{minted}\"")
        );

        let pinned = SurfaceToken::from_string("t1");
        assert_eq!(serde_json::to_string(&pinned).unwrap(), "\"t1\"");
        let back: SurfaceToken = serde_json::from_str("\"t1\"").unwrap();
        assert_eq!(back, pinned);
        assert_eq!(
            SurfaceToken::from_string(String::from("t1")),
            pinned,
            "Eq spans both `Into<String>` forms"
        );
        let mut set = std::collections::HashSet::new();
        assert!(set.insert(pinned.clone()));
        assert!(set.contains(&back));
        assert!(set.insert(SurfaceToken::default()));
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn pre_r30_usage_row_parses_without_typed_field() {
        // Old-session payload: no `usage` key at all. The typed
        // field MUST default to `None` and stay out of the
        // serialised form.
        let row = json!({
            "type": "usage",
            "seq": 4,
            "ts": "2026-09-10T00:00:00Z",
            "data": {
                "prompt_tokens": 900,
                "completion_tokens": 60,
                "total_tokens": 960,
                "cache_read_tokens": 128,
                "cache_write_tokens": 32
            }
        });
        let event = SessionEvent::from_value(&row).expect("old row parses");
        assert_eq!(
            event.provider_usage(),
            Some(UsageBuckets {
                input_tokens: 900,
                output_tokens: 60,
                cache_read_tokens: Some(128),
                cache_write_tokens: Some(32),
            })
        );
        let round = serde_json::to_value(&event).unwrap();
        assert!(round.get("usage").is_none(), "None field is skipped");
    }

    #[test]
    fn typed_usage_field_wins_over_legacy_data() {
        let row = json!({
            "type": "usage",
            "seq": 5,
            "ts": "2026-09-10T00:00:00Z",
            "data": {"prompt_tokens": 1, "completion_tokens": 1},
            "usage": {
                "input_tokens": 700,
                "output_tokens": 40,
                "cache_read_tokens": 12
            }
        });
        let event = SessionEvent::from_value(&row).expect("new row parses");
        let buckets = event.provider_usage().expect("report");
        assert_eq!(buckets.input_tokens, 700);
        assert_eq!(buckets.output_tokens, 40);
        assert_eq!(buckets.cache_read_tokens, Some(12));
        assert_eq!(buckets.cache_write_tokens, None);
    }

    #[test]
    fn usage_row_without_a_decodable_report_has_none_provider_usage() {
        // A usage row whose data carries no `prompt_tokens` is not
        // a provider report; readers must see `None` rather than a
        // fabricated zero sample.
        let row = json!({
            "type": "usage",
            "seq": 6,
            "ts": "2026-09-10T00:00:00Z",
            "data": {"note": "no buckets"}
        });
        let event = SessionEvent::from_value(&row).expect("parses");
        assert!(event.provider_usage().is_none());
        assert!(
            SessionEvent::UserMessage {
                seq: 0,
                ts: String::new(),
                data: json!({}),
                surface_op: None,
            }
            .provider_usage()
            .is_none()
        );
    }

    #[test]
    fn typed_usage_field_round_trips_through_json() {
        let event = SessionEvent::Usage {
            seq: 9,
            ts: "2026-09-10T00:00:00Z".to_string(),
            data: json!({"prompt_tokens": 5}),
            usage: Some(UsageBuckets {
                input_tokens: 5,
                output_tokens: 1,
                cache_read_tokens: None,
                cache_write_tokens: Some(3),
            }),
        };
        let restored = round_trip_value(&event);
        assert_eq!(restored.provider_usage(), event.provider_usage());
        assert_eq!(restored.type_tag(), "usage");
    }

    fn round_trip_value(event: &SessionEvent) -> SessionEvent {
        let value = serde_json::to_value(event).unwrap();
        SessionEvent::from_value(&value).expect("round-trip")
    }
}
