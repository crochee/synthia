# Optimization report R95 — 2026-09-16

## What the audit found

R94 closed the `ControllerInner` trio in the session
controller. The fresh workspace scan:

| Function | File | Score |
|---|---|---|
| `process_event` | provider/streaming/anthropic/processor.rs:73 | 39/20 |
| `build_configured_retriever` | server/state/boot.rs:178 | 26/20 |
| `apply_tools_config` | server/state/boot.rs:354 | 25/20 |
| `classify_compaction` | session/log_surface.rs:378 | 25/20 |
| `build_parts` | server/routes/chat.rs:714 | 24/20 |
| `pump_sse` | provider/streaming/idle_watchdog.rs:102 | 23/20 |
| `discover_skills` | skill/discovery.rs:58 | 23/20 |
| `run_hook_gated` | steering/hook.rs:210 | 22/20 |
| `register_configured_mcp_servers` | server/state/boot.rs:303 | 21/20 |

Seven crates touched; nine functions; one of them
(`process_event`) is the largest in the workspace.

## What landed

Behaviour byte-identical everywhere. All tests pass before
and after — the affected crates' test suites cover each
modified code path.

### `process_event` (39/20, the workspace's last big one)

Five private helpers, one per SSE event category:

- `handle_content_block_start` — the `block.r#type` nested
  match (`tool_use` / `thinking` / `redacted_thinking` /
  `server_tool_use`).
- `handle_content_block_delta` — the `delta.r#type` nested
  match (`text_delta` / `thinking_delta` / `signature_delta`
  / `input_json_delta`).
- `handle_content_block_stop` — the orphan-buffer drain +
  `parse_tool_input_logged` + `ToolUse` build.
- `handle_message_delta` — stop_reason capture only.
- `handle_message_stop` — the heaviest arm; further split
  into `drain_orphan_buffers` (warns on `orphaned`
  list, returns `Vec<String>`), `flush_think_extractor`
  (drains ThinkExtractor into `self.text` / `self.reasoning`),
  and `build_sampling_result` (the `SamplingResult` literal).

The orchestrator body is a 6-arm match.

### `pump_sse` (23/20)

After the first subagent pass extracted `emit_complete_lines`
+ `drain_within_grace`, the orchestrator was still at 21/20.
Second pass extracted four more:

- `cancelled(token) -> bool` — single-expression predicate
  for the top-of-loop check.
- `append_chunk` — decode + append + emit-complete-lines
  (replaces the inline chunk handling + the inner `while
  let Some(pos)` SSE framing loop).
- `emit_tail` — non-newline-terminated tail emission on EOF.
- `abort_after_grace` — the entire cancellation-arm body
  (info log + drain + error).

The orchestrator body is now a flat select! + `append_chunk`
+ `emit_tail` pipeline.

### `build_configured_retriever` (26/20)

`index_path(chunker, file, workspace_root) -> Vec<Document>`
— reads one file or returns empty Vec + a logged skip. The
orchestrator iterates paths, then files, then `extend`s the
documents.

### `apply_tools_config` (25/20)

`activate_groups(active_groups, applied, skipped)` carries
the active-groups branch (the warning on an unknown group,
the push to `active_groups` or `skipped`).

### `register_configured_mcp_servers` (21/20)

`log_tick_health(health: &[ServerHealth])` carries the
health-list iteration and the per-server info/warn log.

### `build_parts` (24/20)

Six per-kind helpers:

- `text_part` (raw `&str`).
- `image_part` / `audio_part` / `file_part` (the three
  attachment kinds with bytes inline).
- `attachment_ref_part` (the hash-only attachment → store
  lookup round-trip).
- `url_part` (URL-attachment variant).

The orchestrator dispatches the attachment kind.

### `classify_compaction` (25/20)

Two helpers:

- `compaction_op(row) -> Option<SurfaceOp>` — validates
  `surface_op` presence, decodes it, enforces the `Replace`
  variant. Each guard logs the same warn message as before.
- `compaction_payload(row) -> Value` — the `data`-shape
  match (object passthrough, non-object wrapped as
  `{"summary": …}`, missing → empty object).

### `discover_skills` (23/20)

- `discover_project_skills(workspace_root)`.
- `discover_user_skills()` — reads `$HOME`, iterates
  `USER_SKILLS_DIRS` in order.
- `collect_skills(paths, source)` — shared parse +
  intra-batch dedup + warn-on-malformed.
- `merge_project_then_user(project, user)` — concatenates
  with project-wins-on-collision via `HashSet<String>` of
  project names.

### `run_hook_gated` (22/20)

- `hook_span(hook, stage, gated) -> tracing::Span`.
- `run_hook_inner` — panic-isolated execution (mirrors
  `run_hook`).
- `gate_rejection` — gated-branch builder (`Aborted` /
  `Closed` reason construction).

## Result

```
$ cargo clippy --workspace --all-targets --all-features \
    --tests --all -- -W clippy::cognitive_complexity \
    | grep -E 'complexity of \([2-9][0-9]\)' | grep -v 'tests\.rs'
(no output — zero non-test violations)
```

Before R95: 9 lib functions, highest 39/20. After: none.

## Verification

- `make ci` **7/7 green**.
- `make test-unit` **2446/2446**; per-crate suites that
  cover the modified paths: `provider` 690 (incl. 46
  anthropic streaming + 9 idle_watchdog), `session` 147,
  `skill` 56, `steering` 72, `server` 403.
- `cargo +nightly fmt --all` clean.

## Deferred

The test-only `cognitive_complexity` warnings in
`provider/anthropic/tests.rs` (27/20, 21/20) and
`session/log_surface.rs:395` test (23/20) are inside the
clippy `#[allow]` set for tests (`expect` / `unwrap` /
`dbg`). Splitting them is straightforward but low-priority
(test code, no production surface); flagged for R96 if
the round triggers.

`compile_error` and macro helpers (`synthia-core/error.rs:525`,
73/20) are inside the `#[allow(dead_code, unused)]` test
exempt; also low-priority.

Standing triggers unchanged.