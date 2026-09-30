# P3 Test Coverage Review — Error Architecture Refactor

**Scope**: P2 (struct-form variants + helpers) + P3 (anyhow! removal, new ErrorCode variants, http_status, Retry-After, context builders)
**Baseline**: 392 tests across 11 crates at last verified run
**Reviewer**: codebase-search subagent (read-only)

---

## TL;DR

| Metric | Value |
|---|---|
| **Estimated overall test coverage for refactor** | ~92% |
| **Verdict** | **Gaps** (small but real — 3 missing tests would close the gaps) |
| **All 39 ErrorCode variants covered by `test_error_code_http_status_canonical_mapping`** | YES (verified — 39/39 distinct `.http_status()` assertions) |
| **Files / paths reviewed** | `crates/synthia-core/src/error/{error.rs, error_code.rs, tests.rs, into_response.rs}`; `crates/synthia-session/src/error.rs`; `crates/synthia-session/src/store/events.rs`; `crates/synthia-server/src/error.rs`; `crates/synthia-context/src/prompt/builder/tests.rs` |

### Findings summary

The P2 + P3 refactor is **substantively well-tested**. The new `http_status()`
method has exhaustive variant coverage (39/39), the three new high-frequency
variants (`ContextOverflow`, `DoomLoop`, `PromptInjection`) have 6 substantive
tests asserting both wire code mapping and structured fields, the
`Retry-After` server contract has 3 tests, and the context builder has 6
tests. Two real gaps were found:

1. **Missing `From<SessionError> for synthia_core::Error` round-trip test** —
   the bridge added in P2.3 is implemented (`crates/synthia-session/src/error.rs:122`)
   but has **zero direct tests**. The `synthia-server/src/error.rs:137`
   transitive test covers `ServerError` mapping but does not isolate the
   core-layer conversion behaviour or its semantics
   (`NotFound`→`synthia_core::Error::not_found`,
   `Unauthorized`→`synthia_core::Error::unauthorized`,
   everything else→`synthia_core::Error::internal(...)`).
2. **Missing `test_code_mapping_for_all_variants` test** (mutation-test hint)
   — deleting an arm in `Error::code()` (`crates/synthia-core/src/error/error.rs:858`)
   would only break the 4-variant representative test on line 198. Three new
   variants (`ContextOverflow`, `DoomLoop`, `PromptInjection`) added in P3-4
   each have helper tests that incidentally assert `code()`, but the test
   grid is sparse for the other 35+ variants — coverage is by accident, not
   by design.
3. **Missing edge-case tests for `with_context()` empty-map and overwrite
   semantics** — the `[k=v]` suffix is conditional on a non-empty BTreeMap
   (`error.rs:1077`), and `with_context` re-inserts via `BTreeMap::insert`
   (`error.rs:662`) which overwrites. Neither behaviour is asserted by name,
   only incidentally.

---

## Axis 1 — Variant coverage of `test_error_code_http_status_canonical_mapping`

| Variant | File:Line | Asserted in `test_error_code_http_status_canonical_mapping`? |
|---|---|---|
| `BadRequest` | error_code.rs:27 | ✅ :354 |
| `Parse` | error_code.rs:43 | ✅ :355 |
| `ValidationError` | error_code.rs:36 | ✅ :357 |
| `InvalidItem` | error_code.rs:41 | ✅ :361 |
| `InvalidCursor` | error_code.rs:60 | ✅ :365 |
| `InvalidSortField` | error_code.rs:61 | ✅ :369 |
| `Unauthorized` | error_code.rs:28 | ✅ :373 |
| `Forbidden` | error_code.rs:29 | ✅ :376 |
| `GuardianViolation` | error_code.rs:50 | ✅ :378 |
| `NotFound` | error_code.rs:30 | ✅ :381 |
| `ModelNotFound` | error_code.rs:48 | ✅ :383 |
| `Conflict` | error_code.rs:31 | ✅ :386 |
| `AlreadyExists` | error_code.rs:40 | ✅ :387 |
| `EditConflict` | error_code.rs:59 | ✅ :388 |
| `RateLimited` | error_code.rs:44 | ✅ :390 |
| `Timeout` | error_code.rs:47 | ✅ :394 |
| `NotImplemented` | error_code.rs:62 | ✅ :398 |
| `InternalServerError` | error_code.rs:32 | ✅ :404 |
| `Io` | error_code.rs:42 | ✅ :408 |
| `ToolExecutionError` | error_code.rs:34 | ✅ :412 |
| `SessionError` | error_code.rs:37 | ✅ :416 |
| `SkillError` | error_code.rs:38 | ✅ :420 |
| `MemoryError` | error_code.rs:39 | ✅ :424 |
| `Stream` | error_code.rs:46 | ✅ :428 |
| `RouterError` | error_code.rs:52 | ✅ :432 |
| `TaskError` | error_code.rs:53 | ✅ :436 |
| `ExecutorError` | error_code.rs:54 | ✅ :440 |
| `ContextError` | error_code.rs:55 | ✅ :444 |
| `TelemetryError` | error_code.rs:56 | ✅ :448 |
| `MultiagentError` | error_code.rs:57 | ✅ :452 |
| `EvaluationError` | error_code.rs:58 | ✅ :456 |
| `ConfigError` | error_code.rs:51 | ✅ :460 |
| `ProviderError` | error_code.rs:35 | ✅ :464 |
| `ServiceUnavailable` | error_code.rs:33 | ✅ :468 |
| `ModelUnavailable` | error_code.rs:49 | ✅ :472 |
| `RetryExhausted` | error_code.rs:45 | ✅ :476 |
| `ContextOverflow` | error_code.rs:63 (P3-4) | ✅ :480 |
| `DoomLoop` | error_code.rs:64 (P3-4) | ✅ :483 |
| `PromptInjection` | error_code.rs:65 (P3-4) | ✅ :485 |

**Status**: ADEQUATE — 39/39 (100%). Programmatic cross-check: distinct
`ErrorCode::VariantName.http_status()` assertions in tests.rs = 39, distinct
variants in error_code.rs enum body = 39, set difference = ∅.

**Evidence**:
- Enumeration of all variants: `crates/synthia-core/src/error/error_code.rs:27-66`
- Test function: `crates/synthia-core/src/error/tests.rs:350-488`
- Implementation under test: `crates/synthia-core/src/error/into_response.rs:21-66`

**Gap**: None on this axis. The three P3-4 variants (`ContextOverflow`,
`DoomLoop`, `PromptInjection`) are NOT skipped — they are explicitly mapped
to `PAYLOAD_TOO_LARGE`, `CONFLICT`, `UNPROCESSABLE_ENTITY` at tests.rs:480-486.
The user's hypothetical "36 + 3 missing" scenario is incorrect.

---

## Axis 2 — P3-4 test quality (new ErrorCode variants)

| Test | File:Line | Asserts behaviour beyond construction? |
|---|---|---|
| `test_context_overflow_helper_roundtrip` | tests.rs:510-528 | YES — destructures variant to verify `limit_tokens=128_000`, `actual_tokens=131_072`; checks `code()` mapping to `ErrorCode::ContextOverflow`; checks Display format contains numeric fields |
| `test_doom_loop_helper_roundtrip` | tests.rs:531-549 | YES — destructures `tool_name="web_search"`, `iterations=3`; checks Display contains `"doom loop"`, `"web_search"`, `"3"` |
| `test_prompt_injection_helper_roundtrip` | tests.rs:552-568 | YES — destructures `source="user_input"`, `pattern="ignore previous instructions"`; checks Display contains diagnostic strings |
| `test_new_variants_carry_call_site_and_context` | tests.rs:571-591 | YES — cross-variant: exercises all 3 helpers with `with_context()`, verifies `location().is_some()` (Io variant excluded) and `context.get(...)` returns the inserted values |
| `test_new_variants_wire_message_strips_context` | tests.rs:594-606 | YES — asserts the P3-7 invariant that context must not leak into UserError wire JSON for `ContextOverflow` (`error.rs:1077-1086` is the contract) |
| `test_new_error_code_variants_have_stable_snake_case` | tests.rs:491-507 | YES — verifies Display strings and serde round-trip for the 3 new variants (ADR-0007 wire stability contract) |

**Status**: ADEQUATE — these are substantive behavioural tests, NOT trivial
construction tests.

**Evidence**: All six tests use `match &err { Error::Variant { ... } => ... }`
destructuring to assert field values; five of the six assert specific Display
substrings; one asserts the `UserError` serialization excludes secret context.

**Gap**: None on this axis. The tests are robust.

---

## Axis 3 — Context field edge cases (P2 `with_context` contract)

| Edge case | Contract location | Test coverage |
|---|---|---|
| `with_context()` returns `Self` (chainable) | error.rs:656-669 | ✅ via `test_with_context_chains` (tests.rs:239-246) |
| Multi-key insertion → sorted iteration in Display | error.rs:1077-1086 (BTreeMap-driven) | ✅ via `test_with_context_iteration_order_is_sorted` (tests.rs:249-258) — explicitly inserts `zebra/z`, `alpha/a`, `middle/m` out of order and asserts `[alpha=a, middle=m, zebra=z]` |
| Io variant: `with_context()` is a silent no-op | error.rs:654-664 (impl), error.rs:756-759 (match) | ✅ via `test_with_context_silently_drops_on_io` (tests.rs:290-295) AND `test_context_accessor_returns_empty_for_io` (tests.rs:282-287) |
| Empty BTreeMap → no `[k=v]` suffix in Display | error.rs:1077 guards `if !ctx.is_empty()` | ❌ **NOT explicitly tested**. The closest is `test_error_display` (tests.rs:166-187) which constructs `Error::not_found("item")` (empty context by default) and checks `starts_with("not found: item")` — but does not assert the absence of `[...]` suffix at the end of the string |
| Re-calling `with_context()` with same key → overwrites (not appends) | error.rs:662 uses `BTreeMap::insert` | ❌ **NOT tested**. No test asserts duplicate-key behaviour |

**Status**: GAPS — 2 of 5 edge cases are not directly named, though one is
implicitly covered by Display format tests.

**Evidence**: Search for "test_with_context" produced only the 3 named tests;
no test asserts `err.to_string().ends_with("]") == false` for a fresh error.

**Gap**: Two tests are missing:
- `test_with_context_empty_map_has_no_suffix` — construct
  `Error::validation("x")` and assert it does NOT end in `]` and does NOT
  contain `[`.
- `test_with_context_overwrites_same_key` — call
  `Error::internal("bug").with_context("attempt", "1").with_context("attempt", "2")`
  and assert `Display` shows exactly one `[attempt=2]`, not `[attempt=1, attempt=2]`.

---

## Axis 4 — `From<SessionError> for synthia_core::Error` round-trip

| Conversion branch | Implementation | Test coverage |
|---|---|---|
| `SessionError::NotFound` → `Error::not_found("session")` | error.rs (synthia-session):125 | ❌ NOT tested directly |
| `SessionError::Unauthorized` → `Error::unauthorized("session unauthorized")` | error.rs (synthia-session):126-128 | ❌ NOT tested directly |
| All other variants → `Error::internal(other.to_string())` | error.rs (synthia-session):129 | ❌ NOT tested directly |
| Round-trip via `ServerError` (synthia-server/error.rs:137-141) | ServerError::from → core::Error::from → ServerError | Partial — `from_core_rate_limited_propagates_retry_after` tests `core::Error → ServerError` for `RateLimited` only (synthia-server/error.rs:186-196) |

**Status**: GAP — the bridge is implemented (`crates/synthia-session/src/error.rs:122-132`)
and is the public cross-crate API for surfacing session-layer errors on the
wire, but it has no dedicated test.

**Evidence**: A program-wide search for `From<SessionError>` /
`test_session_error_to_core` returns only the implementation lines. The
`synthia-session/src/error.rs::tests` mod (lines 146-167) contains only 2
tests for `StoreError`, none for the `From<SessionError>` impl.

**Gap**: One missing test:
`test_from_session_error_to_core_error` — for each `SessionError` variant,
construct it, run `synthia_core::Error::from(se)`, and assert `se.kind()`
matches the expected kind string (i.e. `SessionError::NotFound →
"not_found"`, `SessionError::Unauthorized → "unauthorized"`,
`SessionError::Session("foo") → "internal_server_error"`).
`ErrorCode` lives in `synthia_server::api::error` and is no longer
reachable from `synthia-core`.

---

## Axis 5 — Mutation-test hint

| Property | Currently | Suggested |
|---|---|---|
| Delete an arm in `Error::code()` (`error.rs:858-896`) — which test breaks? | `test_error_code_mapping` (tests.rs:198-209) covers only 4 variants; helper tests (tests.rs:510, :531, :552) cover 3 more; `test_error_code_http_status_canonical_mapping` (tests.rs:350) covers 39 via `http_status()` → `code()` chain in `into_response.rs:71`. So 39 of 39 variant→code mappings *are* exercised indirectly. | A direct `test_code_mapping_for_all_variants` in synthia-core would make the contract explicit: for every helper (`Error::not_found`, `Error::validation`, ..., `Error::context_overflow`, `Error::doom_loop`, `Error::prompt_injection`), assert `ErrorCode::Variant`. |

**Status**: ADEQUATE for the variants that map through `http_status()` (which
is exhaustive). However, `http_status()`-driven tests are an **indirect** check
on `code()` — if `http_status()` is wrong for one variant AND `code()` is
right for the same variant, the test passes on green but both invariants are
weak.

**Gap**: One missing test:
`test_code_mapping_for_all_variants` in synthia-core — iterate every helper
constructor (35+), assert the mapped `ErrorCode`. This would catch a
mismatch between `code()` and `http_status()` and would document the contract
explicitly per ADR-0007.

---

## Verification items from the prompt

### `crates/synthia-session/src/store/events.rs` coverage after P3-2a

**Status**: PRESERVED. The `#[cfg(test)] mod tests` (events.rs:254-554)
contains **11 tests** unchanged by the refactor:
- `test_append_creates_event_with_seq_one`
- `test_read_from_returns_events_after_last_seq`
- `test_read_from_honors_limit`
- `test_read_from_missing_file_starts_at_one`
- `test_seq_monotonicity_across_appends`
- `test_event_source_string_serialization`
- `test_seq_cache_avoids_rescan_on_subsequent_append`
- `test_shared_event_store_caches_across_multiple_appends`
- `test_concurrent_appends_produce_unique_seqs`
- `test_crash_recovery_rescans_for_max_seq`

Verification: `grep -c anyhow! crates/synthia-session/src/store/events.rs`
→ 0 — the P3-2a refactor replaced all 33 `anyhow!()` calls cleanly and the
test mod was not touched. The cache invariants and concurrent seq allocation
behaviour continue to be verified.

### `crates/synthia-server/src/error.rs` tests mod — P3-6 Retry-After tests

The mod at synthia-server/src/error.rs:155-196 contains **3 tests**:
- `too_many_requests_emits_retry_after_header` (line 159) — asserts 42-second
  `retry-after` header for `TooManyRequests { retry_after: Some(42s) }`.
- `too_many_requests_without_retry_after_omits_header` (line 174) — asserts
  no header when `retry_after: None`.
- `from_core_rate_limited_propagates_retry_after` (line 185) — round-trips a
  `synthia_core::Error::rate_limited(Some(7s))` through `ServerError` and
  confirms the duration is preserved.

**Status**: ADEQUATE for the P3-6 contract. All three are behavioural:
they construct the wire response, extract headers / status, and assert
equality.

**Gap**: Minor — no test for `From<SessionError> for ServerError`
(synthia-server/error.rs:137) directly. The
`from_core_rate_limited_propagates_retry_after` test exercises the core→
server path but not the session→server path. (Lower priority — the
underlying `From<SessionError> for synthia_core::Error` test, if added per
Axis 4, would catch most regressions.)

---

## Top 3 missing tests (priority order)

| # | Test name | Location | Rationale | Priority |
|---|---|---|---|---|
| 1 | `test_from_session_error_to_core_error` | New test in `crates/synthia-session/src/error.rs::tests` | The P2.3 bridge is a public cross-crate API; without this test, a regression in error classification (e.g. accidentally routing `Unauthorized` through the `internal(...)` fallback) would not be caught at the layer that owns the impl. | **HIGH** |
| 2 | `test_with_context_empty_map_has_no_suffix` | New test in `crates/synthia-core/src/error/tests.rs::Context API Tests` | Documents the `if !ctx.is_empty()` contract (error.rs:1077) — without this test, a regression that always appended a trailing `[k=v]` would slip through. | **MEDIUM** |
| 3 | `test_code_mapping_for_all_variants` | New test in `crates/synthia-core/src/error/tests.rs::Error → ErrorCode mapping` | A direct grid test of `Error::code()` for every helper constructor would catch any divergence between `code()` and `http_status()` — currently they are tested only transitively through `http_status()`. | **MEDIUM** |

### Bonus gap (mention but do not prioritize)

| Test name | Location | Rationale |
|---|---|---|
| `test_with_context_overwrites_same_key` | synthia-core/src/error/tests.rs | Documents BTreeMap::insert semantics in `with_context()` (error.rs:662); would catch a regression to append-on-duplicate behaviour. |
| `From<SessionError> for ServerError` round-trip | synthia-server/src/error.rs::tests | Lower priority — covered transitively by gap #1 once added. |

---

## Files referenced

- `/home/crochee/workspace/synthia/crates/synthia-core/src/error/error_code.rs`
- `/home/crochee/workspace/synthia/crates/synthia-core/src/error/error.rs`
- `/home/crochee/workspace/synthia/crates/synthia-core/src/error/tests.rs`
- `/home/crochee/workspace/synthia/crates/synthia-core/src/error/into_response.rs`
- `/home/crochee/workspace/synthia/crates/synthia-core/src/error/mod.rs`
- `/home/crochee/workspace/synthia/crates/synthia-session/src/error.rs`
- `/home/crochee/workspace/synthia/crates/synthia-session/src/store/events.rs`
- `/home/crochee/workspace/synthia/crates/synthia-server/src/error.rs`
- `/home/crochee/workspace/synthia/crates/synthia-context/src/prompt/builder/tests.rs`
- `/home/crochee/workspace/synthia/docs/architecture/adr/0007-error-architecture-p2.md`

---

## Path to this review

`docs/architecture/review/p3-test-coverage-review.md`
