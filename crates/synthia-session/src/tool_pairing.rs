//! Tool-pairing balance + shadow-price compaction protocol.
//!
//! R5-4 (dsh `compaction/tool-pairing.ts:181-194` + `compaction/
//! compaction-tool-result-pruner/src/index.ts:113-150` parity):
//!
//! Synthia's [`fold_surface`](crate::surface::fold_surface) already
//! validates `SurfaceOp::Replace` provenance (the
//! `BadProvenance` / `RangeOutOfBounds` errors). What it does NOT
//! validate is that a replace straddles a balanced
//! `assistant/tool_call` ↔ `tool/result` boundary — a compaction
//! that cuts in the middle of a tool call pair silently produces
//! an orphan `tool_call` whose result never arrives. The model then
//! either retries forever or hallucinates a fake result.
//!
//! dsh solves this with a `BalanceCache` keyed by session id that
//! tracks `in_progress` (the count of `assistant_message` events
//! with tool-call blocks minus the count of matching `tool_result`
//! events) at every seq. A compaction's
//! `source_event_seqs` must land on a balanced point — i.e. the
//! fold after the replace must have `in_progress == 0`.
//!
//! This module ports the `BalanceCache` as a session-scoped
//! structure plus a `validate_replace_with_balance` predicate.
//! The cache is rebuilt lazily from the event log on
//! the first call (so cold-start replays work) and incrementally
//! updated as new events arrive.

use std::collections::HashMap;

use serde_json::Value;

use crate::events::{ReplaceRange, SessionEvent, SurfaceOp};

/// `in_progress` count at a given seq (assistant/tool_call minus
/// tool/result). Synthia tracks this at every surface-eligible
/// event so a cut point's balance is a one-lookup operation.
pub struct BalanceCache {
    /// seq → `in_progress` count after that seq's projection to
    /// the surface. Computed lazily on the first call to
    /// [`BalanceCache::build`] or [`validate_replace_with_balance`].
    balance_at: HashMap<u64, i64>,
    /// Highest seq the cache has ingested.
    ingested_through: u64,
    /// Current `in_progress` value (used by `ingest_one`).
    current: i64,
}

impl BalanceCache {
    /// New, empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self {
            balance_at: HashMap::new(),
            ingested_through: 0,
            current: 0,
        }
    }

    /// Rebuild the cache by replaying `events`. Idempotent: if
    /// the cache has already ingested some prefix, only the
    /// tail is replayed.
    pub fn ingest_log(&mut self, events: &[SessionEvent]) {
        for event in events {
            self.ingest_one(event);
        }
    }

    /// Ingest exactly one event. Public so callers can keep the
    /// cache hot as new events arrive (avoids re-scanning the
    /// whole log on every check).
    pub fn ingest_one(&mut self, event: &SessionEvent) {
        let seq = event.seq();
        if seq <= self.ingested_through {
            return;
        }
        match event {
            // assistant_message with tool-use blocks increments.
            SessionEvent::AssistantMessage { data, .. } => {
                if let Some(calls) = extract_tool_use_ids(data) {
                    self.current += calls.len() as i64;
                }
            }
            // tool_result decrements — but only if it matches an
            // outstanding call (orphan tool_results cannot
            // decrement below 0; we clamp at 0).
            SessionEvent::ToolResult { data, .. }
                if extract_tool_use_id(data).is_some() =>
            {
                self.current = (self.current - 1).max(0);
            }
            // Replacements are validated separately by
            // `fold_surface`; they do not change the balance
            // count (the cut happens atomically).
            _ => {}
        }
        self.balance_at.insert(seq, self.current);
        self.ingested_through = seq;
    }

    /// `in_progress` count after the event with the given seq
    /// has been ingested. Returns `None` if `seq` is outside the
    /// cache's coverage (callers should rebuild first).
    #[must_use]
    pub fn balance_after(&self, seq: u64) -> Option<i64> {
        self.balance_at.get(&seq).copied()
    }

    /// `true` iff the fold has `in_progress == 0` at every seq
    /// between `start_seq` (inclusive) and `end_seq` (exclusive).
    /// Used to validate that a `Replace { start, end }` does not
    /// straddle an open tool-call pair.
    #[must_use]
    pub fn is_balanced_between(&self, start_seq: u64, end_seq: u64) -> bool {
        for (&seq, &balance) in &self.balance_at {
            if seq >= start_seq && seq < end_seq && balance != 0 {
                return false;
            }
        }
        true
    }
}

impl Default for BalanceCache {
    fn default() -> Self {
        Self::new()
    }
}

/// `true` iff the surface at the given seq has
/// `in_progress == 0`. A balanced fold means a `Replace` cut at
/// this position would not orphan an in-progress tool call.
#[must_use]
pub fn tool_pairing_balanced_before(cache: &BalanceCache, seq: u64) -> bool {
    cache.balance_after(seq) == Some(0)
}

/// Reason for a rejected balanced cut.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnbalancedCut {
    /// Seq whose `in_progress` was non-zero.
    pub unbalanced_at: u64,
    /// Start index reported by the cut.
    pub start: usize,
    /// End index reported by the cut.
    pub end: usize,
}

/// Validate a `SurfaceOp::Replace` against the surface plus
/// the tool-pairing balance.
///
/// Returns `Err(UnbalancedCut)` if the replace would straddle an
/// open tool-call pair; the surface fold should then refuse the
/// cut.
pub fn validate_replace_with_balance(
    cache: &BalanceCache,
    self_seq: u64,
    range: ReplaceRange<'_>,
) -> Result<(), UnbalancedCut> {
    // If every seq in `source_event_seqs` is balanced, the cut
    // is safe.
    for &cited in range.source_event_seqs {
        if let Some(balance) = cache.balance_after(cited)
            && balance != 0
        {
            return Err(UnbalancedCut {
                unbalanced_at: cited,
                start: range.start,
                end: range.end,
            });
        }
    }
    // Also check `self_seq` itself (the new event must land on a
    // balanced point).
    if let Some(balance) = cache.balance_after(self_seq)
        && balance != 0
    {
        return Err(UnbalancedCut {
            unbalanced_at: self_seq,
            start: range.start,
            end: range.end,
        });
    }
    Ok(())
}

/// Build a fresh `BalanceCache` by replaying `events` from
/// scratch. Convenience for callers that don't keep the cache
/// hot between folds.
#[must_use]
pub fn build_balance_cache(events: &[SessionEvent]) -> BalanceCache {
    let mut cache = BalanceCache::new();
    cache.ingest_log(events);
    cache
}

/// Convenience: walk `events` and return the seq of any seq whose
/// balance is non-zero. Used by tests + diagnostics.
#[must_use]
pub fn find_unbalanced_seqs(cache: &BalanceCache) -> Vec<u64> {
    let mut out: Vec<u64> = cache
        .balance_at
        .iter()
        .filter_map(|(&seq, &b)| if b != 0 { Some(seq) } else { None })
        .collect();
    out.sort_unstable();
    out
}

/// Validate that a `Compaction` event's `source_event_seqs` all
/// land on balanced points. Returns the first unbalanced seq if
/// any.
pub fn compaction_balanced(
    cache: &BalanceCache,
    compaction: &SessionEvent,
) -> Result<(), UnbalancedCut> {
    let (start, end, source_event_seqs) = match compaction {
        SessionEvent::Compaction {
            surface_op:
                SurfaceOp::Replace {
                    start,
                    end,
                    source_event_seqs,
                },
            ..
        } => (*start, *end, source_event_seqs.clone()),
        _ => return Ok(()),
    };
    let range = ReplaceRange {
        start,
        end,
        source_event_seqs: &source_event_seqs,
    };
    validate_replace_with_balance(cache, compaction.seq(), range)
}

/// Extract tool-call IDs from an assistant message payload.
///
/// `Message.content` is `Vec<ContentPart>`; tool-use parts have
/// shape `{ "type": "tool_use", "id": "...", ... }`. We accept
/// both the canonical `Message` JSON shape and a legacy `parts`
/// alias for forward-compat with sessions written before R4.
fn extract_tool_use_ids(data: &Value) -> Option<Vec<String>> {
    let mut ids = Vec::new();
    collect_tool_use_ids(data, &mut ids);
    if ids.is_empty() { None } else { Some(ids) }
}

fn collect_tool_use_ids(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            // tool_use block: { "type": "tool_use", "id": "..." }
            if matches!(
                map.get("type").and_then(Value::as_str),
                Some("tool_use")
            ) && let Some(id) = map.get("id").and_then(Value::as_str)
            {
                out.push(id.to_string());
            }
            // Recurse into nested arrays / objects so we catch
            // tool-use blocks no matter where they sit in the
            // payload.
            for v in map.values() {
                collect_tool_use_ids(v, out);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_tool_use_ids(item, out);
            }
        }
        _ => {}
    }
}

fn extract_tool_use_id(data: &Value) -> Option<String> {
    // A `tool_result` payload carries a `tool_use_id` field
    // (the matching call's id), not a `type: "tool_use"` block.
    if let Some(id) = data.get("tool_use_id").and_then(Value::as_str) {
        return Some(id.to_string());
    }
    let ids = extract_tool_use_ids(data)?;
    ids.into_iter().next()
}
#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::events::ReplaceRange;

    fn user(seq: u64) -> SessionEvent {
        SessionEvent::UserMessage {
            seq,
            ts: "2026-09-10T00:00:00Z".into(),
            data: json!({"role": "user", "content": "hi"}),
            surface_op: Some(SurfaceOp::append()),
        }
    }

    fn assistant_with_tool_use(seq: u64) -> SessionEvent {
        SessionEvent::AssistantMessage {
            seq,
            ts: "2026-09-10T00:00:00Z".into(),
            data: json!({
                "role": "assistant",
                "content": [{
                    "type": "tool_use",
                    "id": "call_1",
                    "name": "shell",
                    "input": {"command": "ls"}
                }]
            }),
            surface_op: Some(SurfaceOp::append()),
        }
    }

    fn tool_result(seq: u64, call_id: &str) -> SessionEvent {
        SessionEvent::ToolResult {
            seq,
            ts: "2026-09-10T00:00:00Z".into(),
            data: json!({
                "role": "tool",
                "tool_use_id": call_id,
                "content": "ok"
            }),
            surface_op: Some(SurfaceOp::append()),
        }
    }

    #[test]
    fn balanced_session_yields_zero_in_progress_at_every_seq() {
        let events = vec![
            user(1),
            assistant_with_tool_use(2),
            tool_result(3, "call_1"),
        ];
        let cache = build_balance_cache(&events);
        // seq 1: 0 (user only). seq 2: +1 (assistant/tool_use).
        // seq 3: 0 (tool_result).
        assert_eq!(cache.balance_after(1), Some(0));
        assert_eq!(cache.balance_after(2), Some(1));
        assert_eq!(cache.balance_after(3), Some(0));
        assert!(cache.is_balanced_between(3, 4));
    }

    #[test]
    fn unbalanced_session_tracks_in_progress_at_assistant() {
        let events = vec![
            user(1),
            assistant_with_tool_use(2),
            // No tool_result — in_progress must be 1 at seq 2.
        ];
        let cache = build_balance_cache(&events);
        assert_eq!(cache.balance_after(1), Some(0));
        assert_eq!(cache.balance_after(2), Some(1));
    }

    #[test]
    fn ingest_one_is_idempotent() {
        let mut cache = BalanceCache::new();
        cache.ingest_one(&user(1));
        cache.ingest_one(&user(1));
        assert_eq!(cache.ingested_through, 1);
        assert_eq!(cache.balance_after(1), Some(0));
    }

    #[test]
    fn validate_replace_rejects_unbalanced_cut() {
        let events = vec![
            user(1),
            assistant_with_tool_use(2),
            // No tool_result — cut at seq 2 is unbalanced.
        ];
        let cache = build_balance_cache(&events);
        let source_event_seqs: Vec<u64> = vec![2];
        let result = validate_replace_with_balance(
            &cache,
            99,
            ReplaceRange {
                start: 0,
                end: 2,
                source_event_seqs: &source_event_seqs,
            },
        );
        let err = result.expect_err("expected unbalanced cut");
        assert_eq!(err.unbalanced_at, 2);
    }

    #[test]
    fn validate_replace_accepts_balanced_cut() {
        let events = vec![
            user(1),
            assistant_with_tool_use(2),
            tool_result(3, "call_1"),
        ];
        let cache = build_balance_cache(&events);
        let source_event_seqs: Vec<u64> = vec![3];
        let result = validate_replace_with_balance(
            &cache,
            99,
            ReplaceRange {
                start: 0,
                end: 2,
                source_event_seqs: &source_event_seqs,
            },
        );
        assert!(result.is_ok());
    }

    #[test]
    fn orphan_tool_result_does_not_decrement_below_zero() {
        let events = vec![
            user(1),
            // A tool_result with no preceding assistant/tool_call.
            tool_result(2, "orphan"),
        ];
        let cache = build_balance_cache(&events);
        // in_progress must clamp at 0, not go to -1.
        assert_eq!(cache.balance_after(2), Some(0));
    }

    #[test]
    fn find_unbalanced_seqs_returns_open_calls() {
        let events = vec![
            user(1),
            assistant_with_tool_use(2),
            assistant_with_tool_use(3),
            tool_result(4, "call_1"),
        ];
        let cache = build_balance_cache(&events);
        // At seq 2: +1 (in_progress = 1).
        // At seq 3: +1 (in_progress = 2).
        // At seq 4: -1 (in_progress = 1).
        assert_eq!(cache.balance_after(2), Some(1));
        assert_eq!(cache.balance_after(3), Some(2));
        assert_eq!(cache.balance_after(4), Some(1));
        assert_eq!(find_unbalanced_seqs(&cache), vec![2, 3, 4]);
    }
}
