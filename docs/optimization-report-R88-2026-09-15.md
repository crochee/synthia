# Optimization report R88 — 2026-09-15

## What the audit found

The round opened with the standing R87 triggers:

- Reference repos `traitclaw` (2026-03-29) / `pi` (2026-09-09)
  unmoved — parity stays closed.
- No new drift surface (`MUST match` / `MUST stay in sync`
  greps return only historical comments and default-vs-new
  assertions).
- Zero `#[allow((dead_code|unused))]` in library code —
  the R87 discipline holds.
- `make ci` 6/6 green at round start.

With the triggers quiet, the audit went to the known-gaps
table. Every row there is declined-by-design except one real
defect: **a regenerated session renders differently on
`/chat/:id` and `/sessions/:id`**. `regenerate` re-queues the
turn, the durable log holds it twice (`prompt, answer1,
prompt, answer2` — an honest record), the chat page
collapses that client-side (`prompt, answer2`), and the
session-detail page + "Continue chat" project the log
verbatim — prompt twice.

The recorded fix path: "give `Rerun` real replace semantics
via a `surface_op` the fold already understands". The
round landed exactly that.

## The seam that made it cheap

Three facts already in the tree:

1. `fold_log_surface` is the **single** raw-log projection —
   `events_to_messages` (the next run's provider history),
   session search, the regenerate route's prompt recovery,
   and `typed_messages_from_sink` all walk through it, and
   `classify_user_input` already applies each row's
   `surface_op` (line `row_op(row)`).
2. `SurfaceOp::Replace { start, end, source_event_seqs }`
   is validated by provenance (cited seqs must precede the
   replacing row) and range, in one shared `plan_op` the
   fold, the typed converter, and the `SurfaceLedger` all
   call — so a replace on a `UserInput` row keeps the
   ledger (compaction-checkpoint provenance) in step for
   free.
3. The compaction checkpoint already writes exactly this
   shape of replace; a rerun is the same move with a
   different span.

## What landed

### Server (`crates/synthia-server/src/session/controller.rs`)

- `pending_multimodal` grows from an opaque tuple to
  `ParkedPrompt { parts, agent_name, rerun }`. `PromptMulti`
  parks `rerun: false`; `SessionOp::Rerun` parks
  `rerun: true`. Every other consumer of the slot treats
  both kinds identically.
- The run task threads the flag to the persistence block
  and, for a rerun, computes the replace op against the
  **pre-rerun fold** (the rows it just read for
  `sink_history`): the span from the last user-role message
  to the tail of the folded surface, cited by those rows'
  seqs — `rerun_replace_op`. The `UserInput` row it persists
  carries `surface_op`.
- Defensive fallback: no user message in the fold (the
  regenerate route refuses this up front) ⇒ plain append,
  no op.

Chained reruns compose: the second rerun's fold already
shows the first rerun's collapsed surface, so it cites the
*surviving* rows and the fold stays `prompt, newest answer`.

### Client (`synthia-web`)

- `SessionTurn` gains the optional `surface_op` field.
- `historyToChatMessages` tracks each built message's
  creating log ordinal (`sources[i]`); a `UserInput` row
  carrying the op drops the messages its cited seqs shadow
  before appending its own (`dropReplacedTurn`). The
  inclusive min..max seq range also removes sub-agent
  bubbles the replaced answer produced — log-only `Agent`
  envelopes the server fold never indexes.
- Compaction rows still produce no client message and stay
  display-invisible; only the regenerate writer's op is
  consumed.
- The chat page's regenerate splice keeps its shape (stable
  message ids for the local store); its stale "the
  transcript holds the turn twice" rationale is rewritten to
  the new truth.

## Why this is the right story

The gap record named two options: collapse the pair inside
the fold generically (wrong — the fold is also the provider
projection, where `prompt, answer1, prompt` is a legitimate
multi-turn exchange), or give the rerun row real replace
semantics (right). Landed the right one: the collapse is
**data** (a cited, provenance-checked span on one row), not
projection logic, so every consumer — display, provider
history, search, resume — applies the same edit or ignores
the row verbatim, and the durable log still records that a
regenerate happened.

## Verification

- `cargo test -p synthia-server`: **403 lib tests** (400 →
  403), 0 failed. The 3 new tests:
  - `rerun_prompt_row_shadows_the_turn_it_replaces` — seeds
    a completed turn, reruns, asserts the persisted row's op
    is exactly `Replace { start: 0, end: 2, seqs: [1, 2] }`
    and `events_to_messages` folds the log to
    `[prompt, fresh answer]`.
  - `chained_rerun_shadows_the_previous_rerun` — two reruns;
    the fold stays `[prompt, newest answer]`.
  - `rerun_without_a_user_turn_appends_plainly` — no user
    row ⇒ no `surface_op` on the persisted row.
- `cargo +nightly fmt --all` clean; `cargo clippy -p
  synthia-server --all-targets --all-features --tests` zero
  warnings.
- `make check-web` green (tsc, eslint, prettier).
- `make ci` 6/6 green.
- **End-to-end (Playwright MCP, per AGENTS.md §4.2)**:
  server + Vite up, visibility patched; sent
  `Reply with exactly the word: mango`, got the answer,
  clicked Regenerate, got the new answer. Evidence chain:
  - `GET /api/v1/sessions/:id` holds the turn twice; the
    rerun's `UserInput` row carries
    `{"start": 0, "end": 3, "source_event_seqs": [2, 3, 4]}`
    (prompt + both answer rows — a streamed answer lands as
    one row per delta).
  - `/chat/:id` renders **one** prompt bubble + the new
    answer.
  - `/sessions/:id` renders the **same** `prompt, answer`
    (pre-fix: prompt twice).
  - Also observed live, earlier in the round: a rerun
    against a failed turn (rate-limited provider) wrote
    `{"start": 0, "end": 1, "source_event_seqs": [2]}` —
    shadowing just the prompt — and the session-detail page
    rendered one prompt.

The verification provider was switched to deepseek
(`--config config.yaml`, key already in the repo's local
untracked config) because both MiniMax endpoints 429'd the
whole round; `config.yaml` was restored verbatim afterward
and both dev processes stopped.

## Deferred

Standing triggers unchanged:

- Reference repos `traitclaw` / `pi` move → re-run the
  R29/R62 parity audit.
- A new drift surface appears → land via the R76–R80
  pattern.
- An architecture gate fails → land the fix.
- A new `#[allow((dead_code|unused))]` slips into library
  code → re-run the R87 grep.

The known-gaps table loses the regenerate row; every
remaining row is declined-by-design with its reason
recorded.
