//! Raw-log surface projection: fold a JSONL session log — typed
//! events and the legacy envelopes the server has always written —
//! into the ordered list of model-visible messages, applying
//! compaction replacements on the way.
//!
//! [`fold_surface`] folds *typed*
//! [`SessionEvent`]s. A production log is a mix: structural rows are
//! typed, message rows are the legacy `{"type":"UserInput"}` /
//! `{"type":"Model"}` envelopes the controller persisted before the
//! typed layer existed, and compaction checkpoints are typed rows on
//! top of both. The resume path that rebuilds a session's message
//! list reads that mixture as one stream, so this module is that
//! stream's single projection:
//!
//! - [`fold_log_surface`] walks the raw rows in order, normalises
//!   every message-producing row (typed or legacy) to the
//!   `synthia_provider::Message` wire shape, stamps each with its
//!   **1-based log ordinal** as its seq, and applies the
//!   [`SurfaceOp::Replace`] op carried by a `compaction` row.
//! - [`SurfaceLedger`] is the incremental twin of that projection:
//!   the run's writer records every row it appends, and a compaction
//!   checkpoint asks the ledger where the rows it is about to shadow
//!   sit and which seqs identify them.
//!
//! Both share `classify_row`'s vocabulary, so the fold and the
//! ledger cannot disagree about what the surface is.
//!
//! ## Provenance in the raw domain
//!
//! A seq is a row's 1-based position in the log. A writer that
//! appends rows in order (the server's run log does) knows the
//! ordinal at append time and the reader reproduces it, so a
//! `Replace { source_event_seqs }` written by a checkpoint cites
//! real earlier rows — no sequence numbers are invented on either
//! side.
//!
//! ## Failure policy
//!
//! A `Replace` whose range or provenance cannot be validated is
//! *ignored* by [`fold_log_surface`] (with a warning) so a corrupt
//! checkpoint degrades to "replay the shadowed span" instead of
//! aborting a resume. [`try_fold_log_surface`] is the strict
//! variant for consumers that want the violation surfaced (the
//! typed projection does).

use std::sync::Arc;

use parking_lot::Mutex;
use serde_json::Value;
use synthia_provider::{Content, ContentPart, Message, Role};

use crate::{
    events::{ReplaceRange, SessionEvent, SurfaceOp},
    surface::{FoldError, FoldedSurface, fold_surface, validate_replace},
};

/// Which typed event variant a surface row projects to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SurfaceKind {
    User,
    Assistant,
    Tool,
    Compaction,
}

/// One raw log row projected onto the surface vocabulary.
enum LogRow {
    /// A surface-contributing row: the normalised payload it carries,
    /// the typed variant it projects to, its stable key (a
    /// tool-result row's call id; `None` for every other row), and
    /// how it joins the surface.
    Surface {
        kind: SurfaceKind,
        payload: Value,
        key: Option<String>,
        op: SurfaceOp,
    },
    /// Log-only, malformed, or unrecognised — no surface
    /// contribution.
    Ignored,
}

/// How a validated op applies to a surface.
enum Applied {
    /// Append the new row at the tail.
    Append,
    /// Replace `[start, end)` with the new row.
    Replace { start: usize, end: usize },
}

/// Classify one raw row.
///
/// Typed message tags (`user_message` / `assistant_message` /
/// `tool_result`) carry a `synthia_provider::Message`-shaped `data`
/// payload; the legacy envelopes (`Message` / `UserInput` /
/// `Model`) carry the shapes the pre-typed controller wrote. Rows
/// outside the surface vocabulary (iteration / step / usage / …)
/// are log-only.
fn classify_row(row: &Value) -> LogRow {
    let Some(tag) = row.get("type").and_then(Value::as_str) else {
        return LogRow::Ignored;
    };
    match tag {
        "compaction" => classify_compaction(row),
        "user_message" | "assistant_message" | "tool_result" => {
            classify_typed_message(row)
        }
        "Message" => classify_message_envelope(row),
        "UserInput" => classify_user_input(row),
        "Model" => classify_model_part(row),
        _ => LogRow::Ignored,
    }
}

/// The op a row asks for: its own `surface_op` when it carries one,
/// `append` otherwise (older callers did not stamp one).
fn row_op(row: &Value) -> SurfaceOp {
    row.get("surface_op")
        .and_then(|value| {
            serde_json::from_value::<SurfaceOp>(value.clone()).ok()
        })
        .unwrap_or_else(SurfaceOp::append)
}

/// Validate `op` against the current surface and report how it
/// applies.
///
/// This is the single gate the fold, the typed converter, and the
/// ledger share, so all three accept and reject exactly the same
/// ops.
fn plan_op(
    op: &SurfaceOp,
    surface_len: usize,
    max_surface_seq: u64,
    self_seq: u64,
) -> Result<Applied, FoldError> {
    match op {
        SurfaceOp::AppendString(_) => Ok(Applied::Append),
        SurfaceOp::Replace {
            start,
            end,
            source_event_seqs,
        } => {
            validate_replace(
                self_seq,
                ReplaceRange {
                    start: *start,
                    end: *end,
                    source_event_seqs,
                },
                surface_len,
                max_surface_seq,
            )?;
            Ok(Applied::Replace {
                start: *start,
                end: *end,
            })
        }
    }
}

/// Typed message row: `data` is the message payload.
fn classify_typed_message(row: &Value) -> LogRow {
    let data = row.get("data").cloned().unwrap_or(Value::Null);
    match decode_message(&data) {
        Some((message, key)) => {
            let kind = kind_of(message.role);
            message_row(message, kind, key, row_op(row))
        }
        // A typed row whose payload is not a full `Message` (e.g. a
        // repair-synthesised tool result) still names a real row.
        None => classify_fallback_payload(row, &data),
    }
}

/// Decode a message-bearing payload.
///
/// Accepts the exact `Message` wire form and the looser
/// `{"role": …, "content": [<ContentPart>, …]}` form callers write
/// by hand; the returned key is the row's durable identity (a
/// tool-result call id) when the payload carries one.
fn decode_message(data: &Value) -> Option<(Message, Option<String>)> {
    if let Ok(message) = serde_json::from_value::<Message>(data.clone()) {
        let key = message.tool_call_id.clone();
        return Some((message, key));
    }
    let role = role_of(data.get("role").and_then(Value::as_str)?)?;
    let parts: Vec<ContentPart> = data
        .get("content")?
        .as_array()?
        .iter()
        .filter_map(|part| {
            serde_json::from_value::<ContentPart>(part.clone()).ok()
        })
        .collect();
    if parts.is_empty() {
        return None;
    }
    let mut key = data
        .get("tool_call_id")
        .and_then(Value::as_str)
        .map(str::to_string);
    if key.is_none() && role == Role::Tool {
        key = parts.iter().find_map(|part| match part {
            ContentPart::ToolResult(result) => Some(result.tool_use_id.clone()),
            _ => None,
        });
    }
    let message = Message {
        role,
        content: Content::parts(parts),
        tool_call_id: key.clone(),
        ..Message::default()
    };
    Some((message, key))
}

/// The provider role a wire tag names.
fn role_of(tag: &str) -> Option<Role> {
    match tag {
        "user" => Some(Role::User),
        "assistant" => Some(Role::Assistant),
        "tool" => Some(Role::Tool),
        "system" => Some(Role::System),
        _ => None,
    }
}

/// Map a provider role onto the typed surface variant.
fn kind_of(role: Role) -> SurfaceKind {
    match role {
        Role::User | Role::System => SurfaceKind::User,
        Role::Tool => SurfaceKind::Tool,
        _ => SurfaceKind::Assistant,
    }
}

/// `{"type":"Message","data":<Message>}` — the lossless
/// `SessionMemory` envelope.
fn classify_message_envelope(row: &Value) -> LogRow {
    let Some(data) = row.get("data").cloned() else {
        return LogRow::Ignored;
    };
    match serde_json::from_value::<Message>(data) {
        Ok(message) => {
            let key = message.tool_call_id.clone();
            let kind = kind_of(message.role);
            message_row(message, kind, key, row_op(row))
        }
        Err(error) => {
            tracing::warn!(
                target: "synthia.session",
                %error,
                "log surface: dropping malformed Message envelope"
            );
            LogRow::Ignored
        }
    }
}

/// `{"type":"UserInput","data":{"text":…}}` — the controller's
/// user-prompt envelope.
fn classify_user_input(row: &Value) -> LogRow {
    match row
        .get("data")
        .and_then(|data| data.get("text"))
        .and_then(Value::as_str)
    {
        Some(text) => message_row(
            Message::user(text),
            SurfaceKind::User,
            None,
            row_op(row),
        ),
        None => {
            tracing::warn!(
                target: "synthia.session",
                "log surface: dropping malformed UserInput envelope"
            );
            LogRow::Ignored
        }
    }
}

/// `{"type":"Model","data":<ContentPart>}` — one streamed model
/// part per row.
fn classify_model_part(row: &Value) -> LogRow {
    let Some(data) = row.get("data").cloned() else {
        return LogRow::Ignored;
    };
    match serde_json::from_value::<ContentPart>(data) {
        Ok(part) => classify_part(part, row_op(row)),
        Err(error) => {
            tracing::warn!(
                target: "synthia.session",
                %error,
                "log surface: dropping malformed Model envelope"
            );
            LogRow::Ignored
        }
    }
}

/// Project one legacy `ContentPart` onto a message row.
///
/// Tool-result rows carry the call id as their key — that is the
/// identity the compaction manager reports, and the only thing by
/// which a shadowed row can be located without trusting an index.
fn classify_part(part: ContentPart, op: SurfaceOp) -> LogRow {
    match part {
        ContentPart::Text(text) => message_row(
            Message {
                role: Role::Assistant,
                content: Content::Single(ContentPart::Text(text)),
                ..Message::default()
            },
            SurfaceKind::Assistant,
            None,
            op,
        ),
        ContentPart::ToolUse(tool_use) => message_row(
            Message {
                role: Role::Assistant,
                content: Content::Single(ContentPart::ToolUse(tool_use)),
                ..Message::default()
            },
            SurfaceKind::Assistant,
            None,
            op,
        ),
        ContentPart::ToolResult(tool_result) => {
            let key = Some(tool_result.tool_use_id.clone());
            message_row(
                Message {
                    role: Role::Tool,
                    content: Content::Single(ContentPart::ToolResult(
                        tool_result,
                    )),
                    ..Message::default()
                },
                SurfaceKind::Tool,
                key,
                op,
            )
        }
        ContentPart::Image(_)
        | ContentPart::Audio(_)
        | ContentPart::Reasoning(_)
        | ContentPart::Resource(_) => LogRow::Ignored,
    }
}

/// Last-resort normalisation: a payload that does not decode as a
/// `Message` but still carries text or a call id.
fn classify_fallback_payload(row: &Value, data: &Value) -> LogRow {
    let op = row_op(row);
    if let Some(text) = data.get("text").and_then(Value::as_str) {
        return message_row(Message::user(text), SurfaceKind::User, None, op);
    }
    let key = data
        .get("call_id")
        .or_else(|| data.get("tool_call_id"))
        .and_then(Value::as_str);
    match key {
        Some(key) => message_row(
            Message::tool(Content::text(data.to_string()), key),
            SurfaceKind::Tool,
            Some(key.to_string()),
            op,
        ),
        None => LogRow::Ignored,
    }
}

/// A `compaction` row: its `surface_op` must be a `Replace` (the
/// event schema makes any other shape unrepresentable) and its
/// summary becomes the replacement message.
fn classify_compaction(row: &Value) -> LogRow {
    let Some(op) = compaction_op(row) else {
        return LogRow::Ignored;
    };
    let payload = compaction_payload(row);
    LogRow::Surface {
        kind: SurfaceKind::Compaction,
        payload,
        key: None,
        op,
    }
}

/// Validate that `row` carries a `Replace` `surface_op`. Any other
/// shape is unrepresentable by the event schema, so a malformed
/// compaction row is dropped with a warning — the resume then
/// replays the shadowed span instead of breaking the fold.
fn compaction_op(row: &Value) -> Option<SurfaceOp> {
    let op_value = compaction_op_value(row)?;
    parse_replace_op(op_value)
}

/// Pull the raw `surface_op` JSON value off a compaction row,
/// warning when the field is missing.
fn compaction_op_value(row: &Value) -> Option<&Value> {
    let Some(op_value) = row.get("surface_op") else {
        tracing::warn!(
            target: "synthia.session",
            "log surface: compaction row carries no surface_op"
        );
        return None;
    };
    Some(op_value)
}

/// Decode a `surface_op` JSON value as a [`SurfaceOp`] and verify
/// it is a `Replace`. Anything else drops the row with a warning —
/// the same recovery path the missing-field case uses.
fn parse_replace_op(op_value: &Value) -> Option<SurfaceOp> {
    let op = match serde_json::from_value::<SurfaceOp>(op_value.clone()) {
        Ok(op) => op,
        Err(_) => {
            tracing::warn!(
                target: "synthia.session",
                "log surface: compaction row does not carry a replace op"
            );
            return None;
        }
    };
    if !matches!(op, SurfaceOp::Replace { .. }) {
        tracing::warn!(
            target: "synthia.session",
            "log surface: compaction row does not carry a replace op"
        );
        return None;
    }
    Some(op)
}

/// The surface row carries the compaction's own payload. The
/// message decoders normalise `{"summary": …}` into an assistant
/// message (see `synthia_context::events_to_messages`), so the
/// raw and typed folds agree on the payload.
fn compaction_payload(row: &Value) -> Value {
    match row.get("data").cloned() {
        Some(payload) if payload.is_object() => payload,
        Some(payload) => serde_json::json!({ "summary": payload.to_string() }),
        None => serde_json::json!({}),
    }
}

/// Serialize a normalised message into a surface row.
fn message_row(
    message: Message,
    kind: SurfaceKind,
    key: Option<String>,
    op: SurfaceOp,
) -> LogRow {
    match serde_json::to_value(&message) {
        Ok(payload) => LogRow::Surface {
            kind,
            payload,
            key,
            op,
        },
        Err(error) => {
            tracing::warn!(
                target: "synthia.session",
                %error,
                "log surface: dropping unserialisable message row"
            );
            LogRow::Ignored
        }
    }
}

/// Walk `rows` in order and return the folded surface, skipping any
/// replacement that does not validate.
fn fold_lenient(rows: &[Value]) -> FoldedSurface {
    let mut surface = FoldedSurface::empty();
    let mut max_surface_seq: u64 = 0;
    // `seq` counts **event** rows, not slice positions: the sink's
    // metadata header is the first line of a fresh log but is not a
    // row, and the sink's own sequence skips it. Counting it here
    // would put every folded seq one ahead of the `stream_index` the
    // rest of the system speaks.
    let mut seq: u64 = 0;
    for row in rows {
        if crate::jsonl::is_metadata_header_row(row) {
            continue;
        }
        seq += 1;
        let LogRow::Surface { payload, op, .. } = classify_row(row) else {
            continue;
        };
        match plan_op(&op, surface.len(), max_surface_seq, seq) {
            Ok(Applied::Append) => {
                surface.messages.push(payload);
                surface.surface_seqs.push(seq);
                max_surface_seq = max_surface_seq.max(seq);
            }
            Ok(Applied::Replace { start, end }) => {
                surface.messages.splice(start..end, [payload]);
                surface.surface_seqs.splice(start..end, [seq]);
                max_surface_seq = max_surface_seq.max(seq);
            }
            Err(error) => {
                tracing::warn!(
                    target: "synthia.session",
                    seq,
                    %error,
                    "log surface: ignoring an unresolvable replace; the \
                     shadowed span is replayed"
                );
            }
        }
    }
    surface
}

/// Fold a raw session log into the model-visible surface.
///
/// Every message-producing row — typed or legacy — becomes one
/// surface row; the row's seq is its 1-based position among the
/// **event** rows. The sink's metadata header
/// ([`is_metadata_header_row`](crate::jsonl::is_metadata_header_row))
/// is not a row: it is skipped and consumes no ordinal, so a folded
/// seq always equals the `stream_index` the sink assigned that event.
/// A `compaction` row replaces the surface span it cites. A
/// replacement that cannot be validated is skipped with a warning,
/// so the caller always gets a usable surface.
#[must_use]
pub fn fold_log_surface(rows: &[Value]) -> FoldedSurface {
    fold_lenient(rows)
}

/// Strict variant of [`fold_log_surface`]: returns the first
/// provenance or range violation instead of skipping the op.
///
/// # Errors
///
/// Returns [`FoldError::BadProvenance`] when a `compaction` row
/// cites a seq outside the previously-folded surface and
/// [`FoldError::RangeOutOfBounds`] when its `[start, end)` does not
/// fit.
pub fn try_fold_log_surface(
    rows: &[Value],
) -> Result<FoldedSurface, FoldError> {
    fold_surface(&surface_events_from_log(rows))
}

/// Project a raw session log onto the typed surface stream.
///
/// Each message-producing row becomes the matching
/// [`SessionEvent::UserMessage`] / [`SessionEvent::AssistantMessage`]
/// / [`SessionEvent::ToolResult`] carrying the normalised payload,
/// and each `compaction` row becomes a
/// [`SessionEvent::Compaction`]; every event is stamped with its
/// 1-based ordinal among the **event** rows as `seq` (the sink's
/// metadata header is skipped and consumes no ordinal, matching
/// [`fold_log_surface`]). Feeding the result to
/// [`fold_surface`] yields the same surface as
/// [`fold_log_surface`], with provenance violations surfaced instead
/// of skipped.
///
/// This is a projection, not a round-trip: the returned events carry
/// the surface payload, not the row's original wire envelope.
/// Unrecognised rows contribute nothing.
#[must_use]
pub fn surface_events_from_log(rows: &[Value]) -> Vec<SessionEvent> {
    let mut events = Vec::with_capacity(rows.len());
    // Same event-only numbering as [`fold_lenient`] so the strict and
    // lenient folds agree on every seq (see that function for why the
    // metadata header does not consume an ordinal).
    let mut seq: u64 = 0;
    for row in rows {
        if crate::jsonl::is_metadata_header_row(row) {
            continue;
        }
        seq += 1;
        let ts = row
            .get("ts")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let LogRow::Surface {
            kind, payload, op, ..
        } = classify_row(row)
        else {
            continue;
        };
        events.push(match kind {
            SurfaceKind::User => SessionEvent::UserMessage {
                seq,
                ts,
                data: payload,
                surface_op: Some(op),
            },
            SurfaceKind::Assistant => SessionEvent::AssistantMessage {
                seq,
                ts,
                data: payload,
                surface_op: Some(op),
            },
            SurfaceKind::Tool => SessionEvent::ToolResult {
                seq,
                ts,
                data: payload,
                surface_op: Some(op),
            },
            SurfaceKind::Compaction => SessionEvent::Compaction {
                seq,
                ts,
                data: payload,
                surface_op: op,
            },
        });
    }
    events
}

/// Where one compaction's shadowed rows sit, and which seqs
/// identify them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedReplace {
    /// Inclusive surface index of the first shadowed row.
    pub start: usize,
    /// Exclusive surface index of the shadowed span.
    pub end: usize,
    /// Seqs of the shadowed rows, in surface order.
    pub source_event_seqs: Vec<u64>,
}

/// Why a compaction's shadowed rows could not be mapped onto the
/// durable log.
///
/// Every variant names the gap so the checkpoint can log exactly
/// what was missing instead of writing provenance it cannot back.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MappingGap {
    /// The record's span size disagrees with the number of keys it
    /// carries.
    #[error(
        "compaction claims {expected} shadowed rows but carries {found} keys"
    )]
    CountMismatch {
        /// Rows the record says it replaced (`end - start`).
        expected: usize,
        /// Keys the record carries.
        found: usize,
    },
    /// The message at `position` has no durable identity, so no log
    /// row can be proven to be it.
    #[error("shadowed row {position} has no durable key")]
    MissingKey {
        /// Offset of the keyless row inside the record.
        position: usize,
    },
    /// No surface row carries the key — the row was never persisted
    /// (yet), so citing it would invent provenance.
    #[error("no durable row carries key {key:?}")]
    UnknownKey {
        /// The key that did not resolve.
        key: String,
    },
    /// More than one surface row carries the key; the shadowed row
    /// is ambiguous.
    #[error("key {key:?} matches more than one durable row")]
    AmbiguousKey {
        /// The ambiguous key.
        key: String,
    },
    /// The keys resolved to rows that are not adjacent in the
    /// surface, so a single `[start, end)` cannot cover them.
    #[error("shadowed rows [{first}, {last}] are not contiguous")]
    NotContiguous {
        /// Surface index of the first resolved row.
        first: usize,
        /// Surface index of the last resolved row.
        last: usize,
    },
}

/// Incremental index of the folded surface over a raw log.
///
/// The run's writer [`record`](Self::record)s every row it appends,
/// in order, with that row's 1-based log ordinal. The ledger keeps
/// one slot — `(seq, key)` — per surface row and applies
/// `compaction` rows exactly like [`fold_log_surface`], so
/// [`resolve`](Self::resolve) answers with a span and seqs the
/// durable fold will reproduce.
///
/// Interior mutability (`Mutex`) because the writer, the checkpoint,
/// and the flush share one ledger behind an `Arc`.
#[derive(Debug, Default)]
pub struct SurfaceLedger {
    state: Mutex<LedgerState>,
}

#[derive(Debug, Default)]
struct LedgerState {
    slots: Vec<LedgerSlot>,
    max_surface_seq: u64,
}

#[derive(Debug, Clone)]
struct LedgerSlot {
    seq: u64,
    key: Option<String>,
}

impl SurfaceLedger {
    /// Empty ledger.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one appended log row. `seq` is the row's 1-based
    /// ordinal in the session log; `row` is the exact value that was
    /// appended.
    ///
    /// Log-only and unrecognised rows contribute nothing. A
    /// `compaction` row whose op does not validate is ignored the
    /// same way [`fold_log_surface`] ignores it, keeping the ledger
    /// and the fold in step.
    pub fn record(&self, seq: u64, row: &Value) {
        let mut state = self.state.lock();
        let LogRow::Surface { key, op, .. } = classify_row(row) else {
            return;
        };
        let slot = LedgerSlot { seq, key };
        match plan_op(&op, state.slots.len(), state.max_surface_seq, seq) {
            Ok(Applied::Append) => state.slots.push(slot),
            Ok(Applied::Replace { start, end }) => {
                state.slots.splice(start..end, [slot]);
            }
            Err(error) => {
                tracing::warn!(
                    target: "synthia.session",
                    seq,
                    %error,
                    "surface ledger: ignoring an unresolvable replace"
                );
                return;
            }
        }
        state.max_surface_seq = state.max_surface_seq.max(seq);
    }

    /// Map a compaction's shadowed-row keys onto the current
    /// surface.
    ///
    /// Every key must resolve to exactly one row, the rows must be
    /// adjacent, and they must appear in the record's order; the
    /// returned span is the one a `Replace` may cite.
    ///
    /// # Errors
    ///
    /// Returns the first [`MappingGap`] that explains why the span
    /// cannot be proven.
    pub fn resolve(
        &self,
        keys: &[Option<String>],
    ) -> Result<ResolvedReplace, MappingGap> {
        let state = self.state.lock();
        let Some((first_key, rest)) = keys.split_first() else {
            return Err(MappingGap::CountMismatch {
                expected: 1,
                found: 0,
            });
        };
        let mut positions = Vec::with_capacity(keys.len());
        let mut seqs = Vec::with_capacity(keys.len());
        for (position, key) in
            std::iter::once(first_key).chain(rest).enumerate()
        {
            let Some(key) = key.as_deref() else {
                return Err(MappingGap::MissingKey { position });
            };
            let mut matches = state
                .slots
                .iter()
                .enumerate()
                .filter(|(_, slot)| slot.key.as_deref() == Some(key));
            let Some((index, slot)) = matches.next() else {
                return Err(MappingGap::UnknownKey {
                    key: key.to_string(),
                });
            };
            if matches.next().is_some() {
                return Err(MappingGap::AmbiguousKey {
                    key: key.to_string(),
                });
            }
            positions.push(index);
            seqs.push(slot.seq);
        }
        let (Some(&start), Some(&last)) = (positions.first(), positions.last())
        else {
            return Err(MappingGap::CountMismatch {
                expected: 1,
                found: 0,
            });
        };
        for window in positions.windows(2) {
            if window[1] != window[0] + 1 {
                return Err(MappingGap::NotContiguous { first: start, last });
            }
        }
        Ok(ResolvedReplace {
            start,
            end: start + positions.len(),
            source_event_seqs: seqs,
        })
    }

    /// Number of surface rows currently indexed.
    #[must_use]
    pub fn len(&self) -> usize {
        self.state.lock().slots.len()
    }

    /// `true` when no surface row has been recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Shared handle to a ledger.
pub type SharedSurfaceLedger = Arc<SurfaceLedger>;

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::{
        fold_log_surface,
        surface_events_from_log,
        try_fold_log_surface,
    };
    use crate::jsonl::SessionMetadataHeader;

    /// The sink's metadata header exactly as the writer emits it.
    fn header() -> Value {
        serde_json::to_value(SessionMetadataHeader::fresh("s1"))
            .expect("the header serialises")
    }

    /// A log whose first line is the sink's metadata header, then the
    /// same event rows as the headerless form.
    fn headered(events: &[Value]) -> Vec<Value> {
        let mut rows = vec![header()];
        rows.extend_from_slice(events);
        rows
    }

    fn plain_events() -> Vec<Value> {
        vec![
            json!({"type": "UserInput", "data": {"text": "hello"}}),
            json!({"type": "Model", "data": {"type": "text", "text": "hi"}}),
        ]
    }

    /// The header is not a row: it contributes no surface entry and,
    /// critically, consumes no ordinal. Numbering slice positions
    /// instead would put every folded seq one ahead of the
    /// `stream_index` the sink assigned that same event.
    #[test]
    fn a_metadata_header_does_not_shift_surface_seqs() {
        let bare = fold_log_surface(&plain_events());
        let with_header = fold_log_surface(&headered(&plain_events()));

        assert_eq!(
            with_header.messages, bare.messages,
            "the header contributes nothing to the surface",
        );
        assert_eq!(
            with_header.surface_seqs,
            vec![1, 2],
            "seqs must count events, not slice positions",
        );
        assert_eq!(with_header.surface_seqs, bare.surface_seqs);
    }

    /// The strict and lenient folds must agree on numbering, or a
    /// caller that switches between them (`try_fold_log_surface` is
    /// the error-surfacing variant) would see different seqs for the
    /// same log.
    #[test]
    fn strict_and_lenient_folds_agree_on_headered_seqs() {
        let rows = headered(&plain_events());
        let lenient = fold_log_surface(&rows);
        let strict = try_fold_log_surface(&rows).expect("the log folds");

        assert_eq!(strict.surface_seqs, vec![1, 2]);
        assert_eq!(strict.surface_seqs, lenient.surface_seqs);
        assert_eq!(strict.messages, lenient.messages);
    }

    /// The typed projection carries the same event-only ordinals, so
    /// feeding it to `fold_surface` reproduces the raw fold.
    #[test]
    fn typed_projection_skips_the_header_ordinal() {
        let rows = headered(&plain_events());
        let events = surface_events_from_log(&rows);
        let seqs: Vec<u64> = events
            .iter()
            .map(|event| match event {
                crate::events::SessionEvent::UserMessage { seq, .. }
                | crate::events::SessionEvent::AssistantMessage {
                    seq, ..
                }
                | crate::events::SessionEvent::ToolResult { seq, .. }
                | crate::events::SessionEvent::Compaction { seq, .. } => *seq,
                other => panic!("unexpected projection: {other:?}"),
            })
            .collect();

        assert_eq!(seqs, vec![1, 2]);
    }

    /// A compaction `Replace` cites the source seqs the durable
    /// checkpoint resolved, and those come from the sink's own
    /// numbering (header-excluded). The header therefore must not
    /// shift the fold's `max_surface_seq`, or the provenance check
    /// would be comparing two different seq spaces.
    #[test]
    fn compaction_provenance_resolves_identically_with_a_header() {
        let events = vec![
            json!({"type": "UserInput", "data": {"text": "hello"}}),
            json!({"type": "Model", "data": {"type": "text", "text": "working"}}),
            json!({
                "type": "compaction",
                "ts": "2026-09-25T00:00:00Z",
                "data": {"summary": "condensed"},
                "surface_op": {
                    "start": 1,
                    "end": 2,
                    "source_event_seqs": [2],
                },
            }),
        ];

        let bare = fold_log_surface(&events);
        let with_header = fold_log_surface(&headered(&events));

        assert_eq!(
            with_header.surface_seqs, bare.surface_seqs,
            "the header must not shift the compaction's own seq either",
        );
        assert_eq!(
            with_header.surface_seqs,
            vec![1, 3],
            "the replace leaves the user row at 1 and lands at 3",
        );
        assert_eq!(with_header.messages, bare.messages);
    }
}
