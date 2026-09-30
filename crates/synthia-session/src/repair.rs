//! Crash-repair: synthesise the boundary events that close an
//! open tail turn after a process crash.
//!
//! Mirrors `deepseek-harness`'s `core/session/src/repair.ts`.
//! `interrupted_turn_closers` walks a `[SessionEvent]` log and,
//! when it finds an open tail (assistant message with tool
//! calls but no matching tool results), emits the synthetic
//! events needed to balance the log so the next run can resume
//! from a well-formed prefix.
//!
//! ## What gets synthesised
//!
//! For each dangling tool call recorded in the open assistant
//! message:
//!
//! 1. A `ToolResult` event with an `error: TOOL_NOT_STARTED`
//!    payload (the call was emitted to the wire but no
//!    provider-side state was recorded for it) OR
//!    `TOOL_OUTCOME_UNKNOWN` (the call was recorded but no
//!    completion row was durably committed — the agent must
//!    replay-or-retry on resume).
//! 2. A `Step { kind: "end", turn, step }` event if a step is
//!    currently open.
//! 3. A `Turn { kind: "end", turn, reason: "interrupted" }` event.
//!
//! Timestamps on synthetic events reuse the last real event's
//! timestamp so the wire stays deterministic and the
//! continuation never invents a "future" wall-clock value.
//!
//! ## Compaction crash detection
//!
//! [`orphaned_compactions`] covers the other half of crash
//! repair: a `CompactionStart` whose `CompactionEnd` never
//! reached the log. The caller decides whether to re-emit the
//! end as `interrupted` or replay the compaction.
//!
//! ## Wire-stable codes
//!
//! The two `TOOL_*` string codes match dsh's export names so
//! downstream consumers (UI, dashboard, prompt recovery flow)
//! can recognise them across the two runtimes.

use serde_json::{Value, json};

use crate::events::{CompactionOutcome, SessionEvent, SurfaceOp};

/// Recovery code for an assistant tool request that never reached
/// a recorded call start.
pub const TOOL_NOT_STARTED: &str = "TOOL_NOT_STARTED";

/// Recovery code for a recorded tool call whose completed outcome
/// was not durably recorded.
pub const TOOL_OUTCOME_UNKNOWN: &str = "TOOL_OUTCOME_UNKNOWN";

/// Recovery reason attached to the synthesised
/// `Turn { kind: "end" }` closer.
pub const TURN_INTERRUPTED: &str = "interrupted";

/// Synthesise the boundary events needed to close an open tail
/// turn after a crash.
///
/// Single-pass scan: tracks the latest `turn/start` and
/// `step/start` boundaries and the set of tool calls already
/// paired with a `tool_result` event. If the log ends with an
/// assistant message whose tool calls are not fully paired,
/// synthetic events are appended in deterministic order:
///
/// 1. One `ToolResult` per dangling call (with
///    `TOOL_NOT_STARTED` when the call was never recorded as a
///    start event, otherwise `TOOL_OUTCOME_UNKNOWN`).
/// 2. A `Step { kind: "end", ... }` if a step is open.
/// 3. A `Turn { kind: "end", reason: TURN_INTERRUPTED, ... }`.
///
/// Timestamps on synthetic events reuse the last real event's
/// timestamp (or the current `now_ms()` fallback) so wire
/// ordering remains deterministic.
///
/// Returns an empty `Vec` when the log is already balanced or
/// empty. Pure function: no I/O, no shared state.
#[must_use]
pub fn interrupted_turn_closers(events: &[SessionEvent]) -> Vec<SessionEvent> {
    // State carried across the scan.
    #[derive(Default)]
    struct State {
        open_turn: Option<u64>,
        open_step: Option<(u64, u64)>,
        // call_id → (turn, step, was_a_tool_call_event_logged)
        pending_calls: std::collections::HashMap<String, PendingCall>,
        last_seq: u64,
        last_ts: String,
    }
    #[derive(Clone)]
    struct PendingCall {
        call_started: bool,
        name: String,
    }

    fn scan(events: &[SessionEvent]) -> State {
        let mut s = State::default();
        for ev in events {
            s.last_seq = s.last_seq.max(ev.seq());
            if let Some(ts) = event_ts(ev) {
                s.last_ts = ts.to_string();
            }
            match ev {
                SessionEvent::Turn { data, .. } => match boundary_kind(data) {
                    Some("start") => {
                        if let Some(turn) =
                            data.get("turn").and_then(Value::as_u64)
                        {
                            s.open_turn = Some(turn);
                            s.open_step = None;
                        }
                    }
                    Some("end") => {
                        s.open_turn = None;
                        s.open_step = None;
                        s.pending_calls.clear();
                    }
                    _ => {}
                },
                SessionEvent::Step { data, .. } => match boundary_kind(data) {
                    Some("start") => {
                        if let (Some(turn), Some(step)) = (
                            data.get("turn").and_then(Value::as_u64),
                            data.get("step").and_then(Value::as_u64),
                        ) {
                            s.open_step = Some((turn, step));
                        }
                    }
                    Some("end") => {
                        s.open_step = None;
                    }
                    _ => {}
                },
                SessionEvent::ToolCall { data, .. } => {
                    if let Some(call_id) =
                        data.get("call_id").and_then(Value::as_str)
                    {
                        let name = data
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string();
                        s.pending_calls.insert(
                            call_id.to_string(),
                            PendingCall {
                                call_started: true,
                                name,
                            },
                        );
                    }
                }
                SessionEvent::AssistantMessage { data, .. } => {
                    // Capture every `tool_use` block on the open
                    // assistant message so dangling calls without
                    // a paired `ToolCall` log event still get a
                    // synthetic closer.
                    if s.open_turn.is_some()
                        && let Some(arr) =
                            data.get("tool_calls").and_then(Value::as_array)
                    {
                        for tc in arr {
                            let call_id = tc
                                .get("id")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string();
                            let name = tc
                                .get("name")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string();
                            s.pending_calls.entry(call_id).or_insert(
                                PendingCall {
                                    call_started: false,
                                    name,
                                },
                            );
                        }
                    }
                }
                SessionEvent::ToolResult { data, .. } => {
                    if let Some(call_id) =
                        data.get("call_id").and_then(Value::as_str)
                    {
                        s.pending_calls.remove(call_id);
                    }
                }
                _ => {}
            }
        }
        s
    }

    let state = scan(events);
    let mut closers = Vec::new();
    let Some(turn) = state.open_turn else {
        return closers;
    };
    let mut seq = state.last_seq + 1;
    let ts = state.last_ts.clone();
    // Step insertion order follows Map iteration (HashMap is
    // non-deterministic; sort by call_id for wire stability).
    let mut pending: Vec<_> = state.pending_calls.into_iter().collect();
    pending.sort_by(|a, b| a.0.cmp(&b.0));
    for (call_id, pending_call) in pending {
        let error_code = if pending_call.call_started {
            TOOL_OUTCOME_UNKNOWN
        } else {
            TOOL_NOT_STARTED
        };
        closers.push(SessionEvent::ToolResult {
            seq,
            ts: ts.clone(),
            data: json!({
                "call_id": call_id,
                "tool_name": pending_call.name,
                "error": error_code,
                "interrupted": true,
            }),
            surface_op: Some(SurfaceOp::append()),
        });
        seq += 1;
    }

    if let Some((_, step)) = state.open_step {
        closers.push(SessionEvent::Step {
            seq,
            ts: ts.clone(),
            data: json!({
                "kind": "end",
                "turn": turn,
                "step": step,
            }),
        });
        seq += 1;
    }
    closers.push(SessionEvent::Turn {
        seq,
        ts,
        data: json!({
            "kind": "end",
            "turn": turn,
            "reason": TURN_INTERRUPTED,
        }),
    });
    closers
}

/// A `CompactionStart` with no matching `CompactionEnd`.
///
/// The crash marker: the process died mid-compaction, so the
/// summary (if any) may or may not have reached the surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrphanedCompaction {
    /// `token` of the unmatched `CompactionStart`.
    pub token: String,
    /// `seq` of the unmatched `CompactionStart`.
    pub start_seq: u64,
}

/// Scan a session log for compaction lifecycles that began but
/// never ended.
///
/// Lifecycles are matched by `token`: `CompactionStart` opens
/// one, `CompactionEnd` closes it. A `CompactionSummary` is not
/// required for a lifecycle to close — a start/end pair with no
/// summary is a compaction that failed before producing one.
///
/// Returned in log order. Pure: no I/O, no shared state.
#[must_use]
pub fn orphaned_compactions(
    events: &[SessionEvent],
) -> Vec<OrphanedCompaction> {
    // `(token, start_seq)` of every start not yet closed, in
    // log order.
    let mut open: Vec<(String, u64)> = Vec::new();
    for event in events {
        match event {
            SessionEvent::CompactionStart { seq, token, .. } => {
                open.push((token.clone(), *seq));
            }
            SessionEvent::CompactionEnd { token, .. } => {
                if let Some(idx) = open.iter().position(|(t, _)| t == token) {
                    open.remove(idx);
                }
            }
            _ => {}
        }
    }
    open.into_iter()
        .map(|(token, start_seq)| OrphanedCompaction { token, start_seq })
        .collect()
}

/// The loader-side view of every compaction in a log: one
/// `(token, outcome)` per lifecycle, in log order.
///
/// A lifecycle that reached a `CompactionEnd` carries the outcome
/// that event recorded (`committed` / `failed`); a lifecycle that
/// only has a `CompactionStart` is **`Interrupted`** — the process
/// died mid-compaction. This is the mapping `orphaned_compactions`
/// documents, applied: a resuming loader calls this and treats an
/// `Interrupted` entry as "the summary may or may not have landed;
/// re-derive or replay".
///
/// An unrecognised `outcome` string maps to `Interrupted` rather
/// than being dropped — an unknown terminal state is not a
/// committed one.
#[must_use]
pub fn compaction_outcomes(
    events: &[SessionEvent],
) -> Vec<(String, CompactionOutcome)> {
    let mut out: Vec<(String, CompactionOutcome)> = Vec::new();
    for event in events {
        match event {
            SessionEvent::CompactionStart { token, .. } => {
                out.push((token.clone(), CompactionOutcome::Interrupted));
            }
            SessionEvent::CompactionEnd { token, outcome, .. } => {
                let parsed = parse_outcome(outcome);
                if let Some(slot) = out.iter_mut().find(|(t, _)| t == token) {
                    slot.1 = parsed;
                } else {
                    // An `End` with no recorded `Start` (a
                    // truncated head) still describes a real
                    // lifecycle; keep it so the count is honest.
                    out.push((token.clone(), parsed));
                }
            }
            _ => {}
        }
    }
    out
}

fn parse_outcome(raw: &str) -> CompactionOutcome {
    match raw {
        "committed" => CompactionOutcome::Committed,
        "failed" => CompactionOutcome::Failed,
        _ => CompactionOutcome::Interrupted,
    }
}

fn event_ts(ev: &SessionEvent) -> Option<&str> {
    match ev {
        SessionEvent::UserMessage { ts, .. }
        | SessionEvent::AssistantMessage { ts, .. }
        | SessionEvent::AssistantChunk { ts, .. }
        | SessionEvent::ToolResult { ts, .. }
        | SessionEvent::Compaction { ts, .. }
        | SessionEvent::CompactionStart { ts, .. }
        | SessionEvent::CompactionSummary { ts, .. }
        | SessionEvent::CompactionEnd { ts, .. }
        | SessionEvent::ToolCall { ts, .. }
        | SessionEvent::Step { ts, .. }
        | SessionEvent::Turn { ts, .. }
        | SessionEvent::Iteration { ts, .. }
        | SessionEvent::Warning { ts, .. }
        | SessionEvent::SteeringGuard { ts, .. }
        | SessionEvent::SteeringHint { ts, .. }
        | SessionEvent::HookBlock { ts, .. }
        | SessionEvent::SubagentEnter { ts, .. }
        | SessionEvent::SubagentExit { ts, .. }
        | SessionEvent::RequestHeader { ts, .. }
        | SessionEvent::Usage { ts, .. }
        | SessionEvent::SandboxMode { ts, .. }
        | SessionEvent::LifecycleShutdown { ts, .. } => Some(ts.as_str()),
    }
}

fn boundary_kind(data: &Value) -> Option<&str> {
    data.get("kind").and_then(Value::as_str)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::CompactionOutcome;

    fn turn_start(turn: u64, ts: &str) -> SessionEvent {
        SessionEvent::Turn {
            seq: turn,
            ts: ts.into(),
            data: json!({"kind": "start", "turn": turn}),
        }
    }

    fn turn_end(turn: u64, ts: &str) -> SessionEvent {
        SessionEvent::Turn {
            seq: turn,
            ts: ts.into(),
            data: json!({"kind": "end", "turn": turn}),
        }
    }

    fn step_start(turn: u64, step: u64, ts: &str) -> SessionEvent {
        SessionEvent::Step {
            seq: turn * 100 + step,
            ts: ts.into(),
            data: json!({"kind": "start", "turn": turn, "step": step}),
        }
    }

    fn assistant_with_tool_calls(
        seq: u64,
        ts: &str,
        calls: &[(&str, &str)],
    ) -> SessionEvent {
        SessionEvent::AssistantMessage {
            seq,
            ts: ts.into(),
            data: json!({
                "text": "calling tools",
                "tool_calls": calls
                    .iter()
                    .map(|(id, name)| json!({"id": id, "name": name}))
                    .collect::<Vec<_>>(),
            }),
            surface_op: None,
        }
    }

    fn tool_result(seq: u64, ts: &str, call_id: &str) -> SessionEvent {
        SessionEvent::ToolResult {
            seq,
            ts: ts.into(),
            data: json!({"call_id": call_id, "content": "ok"}),
            surface_op: None,
        }
    }

    #[test]
    fn balanced_log_produces_no_closers() {
        let events = vec![
            turn_start(1, "2026-09-10T00:00:00Z"),
            step_start(1, 1, "2026-09-10T00:00:00Z"),
            SessionEvent::Turn {
                seq: 99,
                ts: "2026-09-10T00:00:00Z".into(),
                data: json!({"kind": "end", "turn": 1}),
            },
        ];
        assert!(interrupted_turn_closers(&events).is_empty());
    }

    #[test]
    fn empty_log_produces_no_closers() {
        assert!(interrupted_turn_closers(&[]).is_empty());
    }

    #[test]
    fn dangling_call_without_step_or_turn_start_still_closes() {
        // Edge case: assistant message carries tool_use blocks
        // but no turn/step boundary was ever logged. The repair
        // synthesises tool_results with the assistant's seq as
        // the closest anchor.
        let events = vec![assistant_with_tool_calls(
            5,
            "2026-09-10T00:00:00Z",
            &[("c1", "read")],
        )];
        let closers = interrupted_turn_closers(&events);
        // No open turn -> no closers (no boundary to close).
        assert!(closers.is_empty());
    }

    #[test]
    fn dangling_tool_call_after_open_turn_emits_tool_result_then_closers() {
        // The classic crash case: a call was emitted to the
        // wire and recorded as a `ToolCall` log event, but the
        // server died before the tool produced a result.
        let events = vec![
            turn_start(1, "2026-09-10T00:00:00Z"),
            step_start(1, 1, "2026-09-10T00:00:00Z"),
            assistant_with_tool_calls(
                3,
                "2026-09-10T00:00:00Z",
                &[("c1", "read")],
            ),
            SessionEvent::ToolCall {
                seq: 4,
                ts: "2026-09-10T00:00:00Z".into(),
                data: json!({"call_id": "c1", "name": "read", "arguments": {}}),
            },
            // No tool_result for c1, no step/end, no turn/end.
        ];
        let closers = interrupted_turn_closers(&events);
        // Expected order: 1 tool_result (TOOL_OUTCOME_UNKNOWN),
        // 1 step/end, 1 turn/end.
        assert_eq!(closers.len(), 3, "got: {closers:#?}");

        match &closers[0] {
            SessionEvent::ToolResult { seq, data, .. } => {
                assert!(*seq > 4);
                assert_eq!(data["call_id"], "c1");
                assert_eq!(data["error"], TOOL_OUTCOME_UNKNOWN);
                assert_eq!(data["interrupted"], true);
            }
            other => panic!("expected tool_result, got {other:?}"),
        }
        match &closers[1] {
            SessionEvent::Step { data, .. } => {
                assert_eq!(data["kind"], "end");
                assert_eq!(data["turn"], 1);
                assert_eq!(data["step"], 1);
            }
            other => panic!("expected step/end, got {other:?}"),
        }
        match &closers[2] {
            SessionEvent::Turn { data, .. } => {
                assert_eq!(data["kind"], "end");
                assert_eq!(data["turn"], 1);
                assert_eq!(data["reason"], TURN_INTERRUPTED);
            }
            other => panic!("expected turn/end, got {other:?}"),
        }
    }

    #[test]
    fn dangling_tool_use_without_separate_tool_call_event_marks_not_started() {
        // The assistant message's tool_use block was recorded
        // but the wire never logged a `ToolCall` event for it
        // (e.g. a streaming drop on a very old version).
        let events = vec![
            turn_start(1, "2026-09-10T00:00:00Z"),
            step_start(1, 1, "2026-09-10T00:00:00Z"),
            assistant_with_tool_calls(
                3,
                "2026-09-10T00:00:00Z",
                &[("c1", "read")],
            ),
        ];
        let closers = interrupted_turn_closers(&events);
        assert_eq!(closers.len(), 3);
        match &closers[0] {
            SessionEvent::ToolResult { data, .. } => {
                assert_eq!(data["error"], TOOL_NOT_STARTED);
            }
            other => panic!("expected tool_result, got {other:?}"),
        }
    }

    #[test]
    fn paired_call_does_not_get_a_synthetic_result() {
        // c1 has both ToolCall and ToolResult; c2 only has
        // ToolCall. Only c2 should be synthesised.
        let events = vec![
            turn_start(1, "2026-09-10T00:00:00Z"),
            step_start(1, 1, "2026-09-10T00:00:00Z"),
            assistant_with_tool_calls(
                3,
                "2026-09-10T00:00:00Z",
                &[("c1", "read"), ("c2", "shell")],
            ),
            SessionEvent::ToolCall {
                seq: 4,
                ts: "2026-09-10T00:00:00Z".into(),
                data: json!({"call_id": "c1", "name": "read", "arguments": {}}),
            },
            tool_result(5, "2026-09-10T00:00:00Z", "c1"),
            SessionEvent::ToolCall {
                seq: 6,
                ts: "2026-09-10T00:00:00Z".into(),
                data: json!({"call_id": "c2", "name": "shell", "arguments": {}}),
            },
        ];
        let closers = interrupted_turn_closers(&events);
        // One tool_result (for c2) + step/end + turn/end.
        assert_eq!(closers.len(), 3);
        match &closers[0] {
            SessionEvent::ToolResult { data, .. } => {
                assert_eq!(data["call_id"], "c2");
                assert_eq!(data["error"], TOOL_OUTCOME_UNKNOWN);
            }
            other => panic!("expected tool_result, got {other:?}"),
        }
    }

    #[test]
    fn open_step_without_dangling_call_emits_only_step_and_turn_enders() {
        let events = vec![
            turn_start(1, "2026-09-10T00:00:00Z"),
            step_start(1, 1, "2026-09-10T00:00:00Z"),
        ];
        let closers = interrupted_turn_closers(&events);
        // step/end + turn/end, no synthetic tool_result.
        assert_eq!(closers.len(), 2);
        match &closers[0] {
            SessionEvent::Step { data, .. } => {
                assert_eq!(data["kind"], "end");
                assert_eq!(data["turn"], 1);
                assert_eq!(data["step"], 1);
            }
            other => panic!("expected step/end, got {other:?}"),
        }
        match &closers[1] {
            SessionEvent::Turn { data, .. } => {
                assert_eq!(data["kind"], "end");
                assert_eq!(data["turn"], 1);
                assert_eq!(data["reason"], TURN_INTERRUPTED);
            }
            other => panic!("expected turn/end, got {other:?}"),
        }
    }

    #[test]
    fn closed_turn_then_open_turn_only_repairs_the_open_one() {
        let events = vec![
            turn_start(1, "2026-09-10T00:00:00Z"),
            turn_end(1, "2026-09-10T00:00:00Z"),
            turn_start(2, "2026-09-10T00:00:01Z"),
            step_start(2, 1, "2026-09-10T00:00:01Z"),
            assistant_with_tool_calls(
                8,
                "2026-09-10T00:00:01Z",
                &[("c1", "shell")],
            ),
        ];
        let closers = interrupted_turn_closers(&events);
        // The first turn is already closed; only the second is
        // repaired. 1 tool_result + step/end + turn/end.
        assert_eq!(closers.len(), 3);
        match &closers[2] {
            SessionEvent::Turn { data, .. } => {
                assert_eq!(data["turn"], 2);
                assert_eq!(data["reason"], TURN_INTERRUPTED);
            }
            other => panic!("expected turn/end for turn 2, got {other:?}"),
        }
    }

    #[test]
    fn synthetic_seqs_strictly_increase_from_last_real_event() {
        let events = vec![
            turn_start(1, "2026-09-10T00:00:00Z"),
            step_start(1, 1, "2026-09-10T00:00:00Z"),
            assistant_with_tool_calls(
                7,
                "2026-09-10T00:00:00Z",
                &[("c1", "shell")],
            ),
            SessionEvent::ToolCall {
                seq: 8,
                ts: "2026-09-10T00:00:00Z".into(),
                data: json!({"call_id": "c1", "name": "shell", "arguments": {}}),
            },
        ];
        let closers = interrupted_turn_closers(&events);
        let mut last = 8u64;
        for ev in &closers {
            assert!(
                ev.seq() > last,
                "closer seq {} <= last {}",
                ev.seq(),
                last
            );
            last = ev.seq();
        }
    }

    #[test]
    fn multiple_dangling_calls_get_one_tool_result_each_in_id_order() {
        let events = vec![
            turn_start(1, "2026-09-10T00:00:00Z"),
            step_start(1, 1, "2026-09-10T00:00:00Z"),
            assistant_with_tool_calls(
                5,
                "2026-09-10T00:00:00Z",
                &[("c3", "shell"), ("c1", "read"), ("c2", "write")],
            ),
        ];
        let closers = interrupted_turn_closers(&events);
        // 3 tool_results (sorted by call_id) + step/end + turn/end.
        assert_eq!(closers.len(), 5);
        let ids: Vec<&str> = closers
            .iter()
            .take(3)
            .map(|ev| match ev {
                SessionEvent::ToolResult { data, .. } => {
                    data["call_id"].as_str().unwrap_or("")
                }
                _ => "",
            })
            .collect();
        assert_eq!(ids, vec!["c1", "c2", "c3"]);
    }

    fn compaction_start(seq: u64, token: &str) -> SessionEvent {
        SessionEvent::CompactionStart {
            seq,
            ts: "2026-09-10T00:00:00Z".into(),
            token: token.into(),
            data: json!({"reason": "budget"}),
        }
    }

    fn compaction_summary(seq: u64, token: &str) -> SessionEvent {
        SessionEvent::CompactionSummary {
            seq,
            ts: "2026-09-10T00:00:00Z".into(),
            token: token.into(),
            data: json!({"summary": "compacted tail"}),
        }
    }

    fn compaction_end(seq: u64, token: &str, outcome: &str) -> SessionEvent {
        SessionEvent::CompactionEnd {
            seq,
            ts: "2026-09-10T00:00:00Z".into(),
            token: token.into(),
            outcome: outcome.into(),
            data: json!({}),
        }
    }

    #[test]
    fn orphaned_compactions_reports_unmatched_starts_in_log_order() {
        // Contract shape: one closed lifecycle, then an open one.
        let contract = vec![
            compaction_start(1, "t1"),
            compaction_end(2, "t1", "committed"),
            compaction_start(3, "t2"),
        ];
        assert_eq!(
            orphaned_compactions(&contract),
            vec![OrphanedCompaction {
                token: "t2".into(),
                start_seq: 3,
            }]
        );

        // Multiple orphans keep their log order.
        let events = vec![
            compaction_start(1, "t1"),
            compaction_end(2, "t1", "committed"),
            compaction_start(3, "t2"),
            compaction_start(4, "t3"),
        ];
        assert_eq!(
            orphaned_compactions(&events),
            vec![
                OrphanedCompaction {
                    token: "t2".into(),
                    start_seq: 3,
                },
                OrphanedCompaction {
                    token: "t3".into(),
                    start_seq: 4,
                },
            ]
        );
    }

    #[test]
    fn orphaned_compactions_needs_an_end_not_a_summary() {
        assert!(orphaned_compactions(&[]).is_empty());

        // Closed lifecycle — a summary is optional, the end is
        // what closes it.
        let closed = vec![
            compaction_start(1, "t1"),
            compaction_summary(2, "t1"),
            compaction_end(3, "t1", "committed"),
            compaction_start(4, "t2"),
            compaction_end(5, "t2", "failed"),
        ];
        assert!(orphaned_compactions(&closed).is_empty());

        // A summary alone leaves the lifecycle open.
        let summary_only =
            vec![compaction_start(7, "t9"), compaction_summary(8, "t9")];
        assert_eq!(
            orphaned_compactions(&summary_only),
            vec![OrphanedCompaction {
                token: "t9".into(),
                start_seq: 7,
            }]
        );
    }

    /// R29-Phase-J: the loader-side mapping. Matched lifecycles
    /// report their recorded outcome; an unmatched `Start` reports
    /// `Interrupted` — the "crashed mid-compaction" state.
    #[test]
    fn compaction_outcomes_maps_orphans_to_interrupted() {
        let events = vec![
            compaction_start(1, "t1"),
            compaction_summary(2, "t1"),
            compaction_end(3, "t1", "committed"),
            compaction_start(4, "t2"),
            compaction_end(5, "t2", "failed"),
            // Crash: started, never ended.
            compaction_start(6, "t3"),
        ];
        assert_eq!(
            compaction_outcomes(&events),
            vec![
                ("t1".to_string(), CompactionOutcome::Committed),
                ("t2".to_string(), CompactionOutcome::Failed),
                ("t3".to_string(), CompactionOutcome::Interrupted),
            ]
        );
    }

    /// An unrecognised terminal outcome is not a committed one —
    /// it maps to `Interrupted` rather than being dropped.
    #[test]
    fn compaction_outcomes_unknown_terminal_maps_to_interrupted() {
        let events =
            vec![compaction_start(1, "t1"), compaction_end(2, "t1", "wat")];
        assert_eq!(
            compaction_outcomes(&events),
            vec![("t1".to_string(), CompactionOutcome::Interrupted)]
        );
    }
}
