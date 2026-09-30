# Optimization report R80 — 2026-09-15

## What the audit found

R77/R78/R79 closed three drift surfaces (char truncator,
partial redactor, cursor codec) by collapsing them into
`synthia-core`. The next grep — `MUST match`,
`MUST stay in sync`, `byte-compatible` — found a fourth,
this time a **constant** rather than a function:

> ```text
> /// Mirrors the wire-level constant in
> /// `synthia_server::api::v1` — the two MUST stay in sync
> /// so the cursor envelope emitted by `synthia-server`
> /// handlers agrees with what `Registry`'s default
> /// cursor decoder produces.
> const REGISTRY_MAX_LIMIT: u64 = 100;
> ```
> — `synthia-core::registry` (private const, 1 reference
> site in `paginate_registry_list`)

The server's matching constant is
`synthia_server::api::v1::page_query::MAX_LIMIT = 100`
(pub const, 8 reference sites including handlers and
tests). The two had the same value but no compiler
enforcement — a refactor that changed one without the
other would silently desync the wire-level cap from the
in-memory registry pagination cap.

The shape is identical to R77/R78/R79: a primitive
that exists in two places with a sync comment, and a
real risk of drift. The right home is the same
`synthia-core` (where the in-memory pagination lives),
with the server aliasing the core's value through a
`pub const X = synthia_core::Y;` line the compiler
enforces.

## What landed

- `synthia_core::registry::MAX_LIMIT` is now `pub const
  MAX_LIMIT: u64 = 100`. The doc comment documents the
  role (in-memory registry pagination cap) and the
  relationship to the server's wire-level cap (the
  server aliases this constant). The previous
  "MUST stay in sync" comment is now a single source
  of truth.
- The internal references (1 in the function body,
  3 in the doc comments of `paginate_registry_list`,
  4 in the existing in-crate test) all renamed from
  `REGISTRY_MAX_LIMIT` to `MAX_LIMIT`. The behaviour
  is unchanged.
- `synthia_core::lib.rs` re-exports `MAX_LIMIT` so the
  server can `use synthia_core::registry::MAX_LIMIT`.
  The module-doc table row for `registry` mentions
  the constant.
- `synthia_server::api::v1::page_query::MAX_LIMIT` is
  now `pub const MAX_LIMIT: u64 =
  synthia_core::registry::MAX_LIMIT;` — the server
  keeps its public symbol name (so downstream
  callers' imports don't change) but its value is
  sourced from core. The doc comment explains the
  alias and the single-source-of-truth contract.
- 1 new test in `synthia-core` (135 total, was 134):
  `max_limit_matches_spec` — pins the value to `100`
  so a refactor that silently changed it would fail
  the in-crate test. The server's existing
  `constants_match_spec` test now also asserts
  `assert_eq!(MAX_LIMIT, synthia_core::registry::
  MAX_LIMIT)` — drift detector for the alias.

## Why a constant, not a function

The R77/R78/R79 pattern was a *function* that existed
in two places; R80 is a *constant* that exists in two
places. The refactor is the same shape (one canonical
home, one consumer-side alias) but the contract is
"both values track" instead of "both behaviours track".
A `pub const X = synthia_core::Y;` line is the
smallest possible refactor that closes the drift:
zero new function, zero new abstraction, one line that
the compiler enforces for the rest of the codebase's
lifetime.

## Verification

- `cargo test -p synthia-core --lib` 135 passed
  (was 134, +1).
- `cargo test -p synthia-server --lib` 400 passed
  (unchanged — the existing `constants_match_spec` test
  still passes, now with one extra assertion that
  pins the alias).
- `make ci` 6/6 green.
- `make test-unit` 2447 passed (was 2446, +1).
- `make examples` exit 0 with `CONSUMER-PROOF: OK` +
  `MVP-OK` + `BEST-OF-N-JUDGE: OK`.
- `make check-mvp-deps` and `make check-public-api-runtime`
  both green: the constant is in `synthia-core`
  (no async, no runtime).

## Deferred

None. The drift surface is closed by a one-line alias
in the server and a one-line test pin in the core.
There is no follow-up maintenance to record.
