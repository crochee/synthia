//! Typed event builders for the ReAct loop's structural
//! boundaries.
//!
//! R6-2 (dsh `turn/start|end` + `step/start|end` parity). The
//! [`SessionEvent`] enum already carries `Step` / `Turn` /
//! `Iteration` / `SubagentEnter` / `SubagentExit` variants, but
//! the loop has had no producer for them. This module pins the
//! wire shape each event takes so the loop and the controller
//! agree on the JSON contract before any of them have to wire
//! up the actual call site.
//!
//! ## Why builders, not direct enum construction?
//!
//! Five reasons:
//!
//! 1. **Centralised `seq` / `ts` stamping** — the loop fills
//!    the structural payload (iteration / step / turn) and the
//!    builder stamps `seq: 0` + `ts: ""`. The controller
//!    re-stamps at append time.
//! 2. **Shape stability** — if the loop constructs a
//!    `SessionEvent::Step` inline, any future schema change
//!    touches every site. The builder is the single
//!    canonicalisation point.
//! 3. **Frontend codegen** — `KNOWN_SESSION_EVENT_TYPES` exposes
//!    the *type* tags; this module exposes the *payload* shape
//!    so the OpenAPI generator can describe each event.
//! 4. **Replay-friendliness** — `fold_surface` /
//!    `interrupted_turn_closers` already understand the
//!    `data`-as-`Value` envelope; builders produce the exact
//!    shape the repair helpers expect.
//! 5. **No engine-level wiring required** — the builders are
//!    pure functions. The agent loop can adopt them
//!    incrementally without touching its hot path; future R7
//!    work wires them into the actual emission points.

use serde_json::{Value, json};

use crate::{events::SessionEvent, token_meter::UsageBuckets};

/// Build a `Step { start }` event. `step_idx` is the 0-based
/// step ordinal inside the current turn; `turn_idx` is the
/// 0-based turn ordinal inside the run.
#[must_use]
pub fn step_start(turn_idx: u32, step_idx: u32) -> SessionEvent {
    SessionEvent::Step {
        seq: 0,
        ts: String::new(),
        data: json!({
            "kind": "start",
            "turn": turn_idx,
            "step": step_idx,
        }),
    }
}

/// Build a `Step { end }` event. Mirrors [`step_start`].
#[must_use]
pub fn step_end(turn_idx: u32, step_idx: u32, action: &str) -> SessionEvent {
    SessionEvent::Step {
        seq: 0,
        ts: String::new(),
        data: json!({
            "kind": "end",
            "turn": turn_idx,
            "step": step_idx,
            "action": action,
        }),
    }
}

/// Build a `Turn { start }` event. The 0-based turn ordinal
/// increments every time the model emits a non-final answer.
#[must_use]
pub fn turn_start(turn_idx: u32) -> SessionEvent {
    SessionEvent::Turn {
        seq: 0,
        ts: String::new(),
        data: json!({
            "kind": "start",
            "turn": turn_idx,
        }),
    }
}

/// Build a `Turn { end }` event. `reason` is one of:
/// `"completed"`, `"max_iterations"`, `"cancelled"`, `"error"`.
#[must_use]
pub fn turn_end(turn_idx: u32, reason: &str) -> SessionEvent {
    SessionEvent::Turn {
        seq: 0,
        ts: String::new(),
        data: json!({
            "kind": "end",
            "turn": turn_idx,
            "reason": reason,
        }),
    }
}

/// Build an `Iteration { start }` event. R6-2 keeps this
/// distinct from `step` to mirror dsh's
/// `iteration/start|end` semantics: an iteration wraps one
/// pass of the LLM, a step wraps one tool dispatch within an
/// iteration.
#[must_use]
pub fn iteration_start(iteration_idx: u32) -> SessionEvent {
    SessionEvent::Iteration {
        seq: 0,
        ts: String::new(),
        data: json!({
            "kind": "start",
            "iteration": iteration_idx,
        }),
    }
}

/// Build an `Iteration { end }` event.
#[must_use]
pub fn iteration_end(iteration_idx: u32, action: &str) -> SessionEvent {
    SessionEvent::Iteration {
        seq: 0,
        ts: String::new(),
        data: json!({
            "kind": "end",
            "iteration": iteration_idx,
            "action": action,
        }),
    }
}

/// Build a `SubagentEnter` event. `child_session_id` is the
/// session id the delegating sub-agent will use; `depth` is the
/// sub-agent depth (1 = direct child, 2 = grandchild, ...).
#[must_use]
pub fn subagent_enter(
    child_session_id: &str,
    parent_session_id: &str,
    depth: u32,
    agent_name: &str,
) -> SessionEvent {
    SessionEvent::SubagentEnter {
        seq: 0,
        ts: String::new(),
        data: json!({
            "child_session_id": child_session_id,
            "parent_session_id": parent_session_id,
            "depth": depth,
            "agent_name": agent_name,
        }),
    }
}

/// Build a `SubagentExit` event. `status` is one of:
/// `"completed"`, `"cancelled"`, `"error"`.
#[must_use]
pub fn subagent_exit(
    child_session_id: &str,
    depth: u32,
    status: &str,
) -> SessionEvent {
    SessionEvent::SubagentExit {
        seq: 0,
        ts: String::new(),
        data: json!({
            "child_session_id": child_session_id,
            "depth": depth,
            "status": status,
        }),
    }
}

/// Build a `RequestHeader` event with the resolved provider +
/// model + tools_hash triple. `reason` is `"initial"` on the
/// first run, `"change"` when the configuration drifted.
#[must_use]
pub fn request_header(
    provider: &str,
    model: &str,
    tools_hash: &str,
    reason: &str,
) -> SessionEvent {
    SessionEvent::RequestHeader {
        seq: 0,
        ts: String::new(),
        data: json!({
            "reason": reason,
            "provider": provider,
            "model": model,
            "tools_hash": tools_hash,
        }),
    }
}

/// Build a `Usage` event for one LLM call. `prompt_tokens` /
/// `completion_tokens` are the required fields;
/// `reasoning_tokens` / `cache_read_tokens` /
/// `cache_write_tokens` are optional and serialised only
/// when present.
///
/// R30: the event carries the provider report twice — the typed
/// `usage` field (the [`crate::token_meter`] fold and new
/// readers) and the legacy `data` payload (older readers) — so
/// a log written by either generation replays on both.
#[must_use]
pub fn usage(
    prompt_tokens: usize,
    completion_tokens: usize,
    total_tokens: usize,
    reasoning_tokens: Option<usize>,
    cache_read_tokens: Option<usize>,
    cache_write_tokens: Option<usize>,
) -> SessionEvent {
    let mut data = json!({
        "prompt_tokens": prompt_tokens,
        "completion_tokens": completion_tokens,
        "total_tokens": total_tokens,
    });
    if let Some(r) = reasoning_tokens {
        data["reasoning_tokens"] = json!(r);
    }
    if let Some(r) = cache_read_tokens {
        data["cache_read_tokens"] = json!(r);
    }
    if let Some(w) = cache_write_tokens {
        data["cache_write_tokens"] = json!(w);
    }
    let usage = UsageBuckets {
        input_tokens: prompt_tokens as u64,
        output_tokens: completion_tokens as u64,
        cache_read_tokens: cache_read_tokens.map(|n| n as u64),
        cache_write_tokens: cache_write_tokens.map(|n| n as u64),
    };
    SessionEvent::Usage {
        seq: 0,
        ts: String::new(),
        data,
        usage: Some(usage),
    }
}

/// Build a `SandboxMode { mode }` event — the durable
/// sandbox-policy override for a session (R31).
///
/// `mode` is the execution-policy wire name (`"read-only"` /
/// `"workspace-write"` / `"danger-full-access"`). It is preserved
/// verbatim rather than validated: this crate is the wire layer
/// and does not know the policy enum, a log written by a newer
/// build must round-trip here, and the policy fold ignores a mode
/// it does not know. The seq is stamped `0` like every builder's —
/// the controller re-stamps at append time.
#[must_use]
pub fn sandbox_mode(mode: &str) -> SessionEvent {
    SessionEvent::SandboxMode {
        seq: 0,
        ts: String::new(),
        mode: mode.to_string(),
    }
}

/// Round-trip a typed event through `serde_json::Value` and
/// back. Builders must produce JSON that survives
/// `from_value` — that is the durability contract.
#[must_use]
pub fn round_trip(event: &SessionEvent) -> Option<SessionEvent> {
    let value = serde_json::to_value(event).ok()?;
    SessionEvent::from_value(&value)
}

/// Extract the `kind` tag from a `Step` / `Turn` / `Iteration`
/// event's data payload, if any. Returns `None` for events
/// that are not structural boundaries.
#[must_use]
pub fn structural_kind(event: &SessionEvent) -> Option<&str> {
    let data = match event {
        SessionEvent::Step { data, .. }
        | SessionEvent::Turn { data, .. }
        | SessionEvent::Iteration { data, .. } => data,
        _ => return None,
    };
    data.get("kind").and_then(Value::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_start_carries_turn_and_step_ordinals() {
        let event = step_start(2, 4);
        let value = serde_json::to_value(&event).unwrap();
        assert_eq!(value["type"], "step");
        assert_eq!(value["data"]["kind"], "start");
        assert_eq!(value["data"]["turn"], 2);
        assert_eq!(value["data"]["step"], 4);
        // seq is 0 — the controller stamps it.
        assert_eq!(value["seq"], 0);
        assert_eq!(value["ts"], "");
    }

    #[test]
    fn step_end_carries_action() {
        let event = step_end(0, 1, "tool_call");
        let value = serde_json::to_value(&event).unwrap();
        assert_eq!(value["data"]["kind"], "end");
        assert_eq!(value["data"]["action"], "tool_call");
    }

    #[test]
    fn turn_start_end_round_trip_via_known_event_types() {
        let start = turn_start(0);
        let end = turn_end(0, "max_iterations");
        assert_eq!(start.type_tag(), "turn");
        assert_eq!(end.type_tag(), "turn");
        // Both round-trip through serde.
        assert!(round_trip(&start).is_some());
        assert!(round_trip(&end).is_some());
    }
    #[test]
    fn iteration_start_end_paired() {
        let start = iteration_start(7);
        let end = iteration_end(7, "completed");
        let value = serde_json::to_value(&end).unwrap();
        assert_eq!(value["data"]["kind"], "end");
        assert_eq!(value["data"]["iteration"], 7);
        assert_eq!(value["data"]["action"], "completed");
        // Pin `start` is constructed without warnings.
        let _ = start.type_tag();
    }

    #[test]
    fn subagent_enter_carries_parent_and_depth() {
        let event = subagent_enter("child-1", "parent-1", 2, "researcher");
        let value = serde_json::to_value(&event).unwrap();
        assert_eq!(value["type"], "subagent_enter");
        assert_eq!(value["data"]["child_session_id"], "child-1");
        assert_eq!(value["data"]["parent_session_id"], "parent-1");
        assert_eq!(value["data"]["depth"], 2);
        assert_eq!(value["data"]["agent_name"], "researcher");
    }

    #[test]
    fn subagent_exit_carries_status() {
        let event = subagent_exit("child-1", 2, "completed");
        let value = serde_json::to_value(&event).unwrap();
        assert_eq!(value["type"], "subagent_exit");
        assert_eq!(value["data"]["status"], "completed");
    }

    #[test]
    fn request_header_round_trips_with_reason() {
        let event = request_header(
            "anthropic",
            "claude-opus-4-7",
            "deadbeef",
            "initial",
        );
        let value = serde_json::to_value(&event).unwrap();
        assert_eq!(value["type"], "request_header");
        assert_eq!(value["data"]["provider"], "anthropic");
        assert_eq!(value["data"]["model"], "claude-opus-4-7");
        assert_eq!(value["data"]["tools_hash"], "deadbeef");
        assert_eq!(value["data"]["reason"], "initial");
        // Round-trip via from_value.
        let restored = round_trip(&event).expect("parses");
        assert_eq!(restored.type_tag(), "request_header");
    }

    #[test]
    fn usage_omits_none_optionals_from_data() {
        // The Usage event must not include `"reasoning_tokens":
        // null` in the serialised form — providers that do not
        // surface reasoning tokens must produce a clean
        // envelope.
        let event = usage(100, 50, 150, None, Some(80), Some(20));
        let value = serde_json::to_value(&event).unwrap();
        assert!(value["data"].get("prompt_tokens").is_some());
        assert!(value["data"].get("completion_tokens").is_some());
        assert!(value["data"].get("total_tokens").is_some());
        assert!(value["data"].get("cache_read_tokens").is_some());
        assert!(value["data"].get("cache_write_tokens").is_some());
        // `reasoning_tokens` is None → not in the payload.
        assert!(value["data"].get("reasoning_tokens").is_none());
    }

    #[test]
    fn usage_includes_reasoning_when_provided() {
        let event = usage(100, 50, 150, Some(7), None, None);
        let value = serde_json::to_value(&event).unwrap();
        assert_eq!(value["data"]["reasoning_tokens"], 7);
        // Optionals not provided must not appear.
        assert!(value["data"].get("cache_read_tokens").is_none());
        assert!(value["data"].get("cache_write_tokens").is_none());
    }

    #[test]
    fn usage_stamps_typed_buckets_matching_legacy_payload() {
        let event = usage(100, 50, 150, Some(7), Some(80), Some(20));
        let value = serde_json::to_value(&event).unwrap();
        // The typed field is the current wire form.
        assert_eq!(value["usage"]["input_tokens"], 100);
        assert_eq!(value["usage"]["output_tokens"], 50);
        assert_eq!(value["usage"]["cache_read_tokens"], 80);
        assert_eq!(value["usage"]["cache_write_tokens"], 20);
        // And it decodes back to the same disjoint report.
        let buckets = event.provider_usage().expect("typed report");
        assert_eq!(buckets.input_tokens, 100);
        assert_eq!(buckets.output_tokens, 50);
        assert_eq!(buckets.cache_read_tokens, Some(80));
        assert_eq!(buckets.cache_write_tokens, Some(20));
        assert_eq!(buckets.prompt_side_tokens(), 200);
    }

    #[test]
    fn sandbox_mode_carries_the_policy_wire_name() {
        let event = sandbox_mode("read-only");
        let value = serde_json::to_value(&event).unwrap();
        assert_eq!(value["type"], "sandbox_mode");
        assert_eq!(value["mode"], "read-only");
        assert_eq!(value["seq"], 0);
        assert_eq!(value["ts"], "");
        let restored = round_trip(&event).expect("round-trips");
        assert_eq!(restored.sandbox_mode(), Some("read-only"));
        // A mode this build does not know survives the log rather
        // than failing it: the fold, not the wire, rejects it.
        let unknown = round_trip(&sandbox_mode("future-mode"))
            .expect("unknown mode parses");
        assert_eq!(unknown.sandbox_mode(), Some("future-mode"));
    }

    #[test]
    fn structural_kind_extracts_kind_tag() {
        assert_eq!(structural_kind(&step_start(0, 0)), Some("start"));
        assert_eq!(structural_kind(&step_end(0, 0, "x")), Some("end"));
        assert_eq!(structural_kind(&turn_start(0)), Some("start"));
        assert_eq!(structural_kind(&turn_end(0, "x")), Some("end"));
        assert_eq!(structural_kind(&iteration_start(0)), Some("start"));
        // Non-structural events return None.
        let non_structural = usage(1, 1, 2, None, None, None);
        assert_eq!(structural_kind(&non_structural), None);
    }

    #[test]
    fn all_builders_round_trip_through_serde() {
        // One test per builder — pins the durability contract
        // for every event the loop might emit.
        let cases: Vec<(&str, SessionEvent)> = vec![
            ("step_start", step_start(0, 0)),
            ("step_end", step_end(0, 0, "completed")),
            ("turn_start", turn_start(0)),
            ("turn_end", turn_end(0, "completed")),
            ("iteration_start", iteration_start(0)),
            ("iteration_end", iteration_end(0, "completed")),
            ("subagent_enter", subagent_enter("c", "p", 1, "a")),
            ("subagent_exit", subagent_exit("c", 1, "completed")),
            ("request_header", request_header("p", "m", "h", "initial")),
            ("usage", usage(1, 1, 2, None, None, None)),
            ("sandbox_mode", sandbox_mode("workspace-write")),
        ];
        for (name, event) in cases {
            let restored = round_trip(&event)
                .unwrap_or_else(|| panic!("{name} must round-trip"));
            assert_eq!(restored.type_tag(), event.type_tag(), "{name}");
        }
    }
}
