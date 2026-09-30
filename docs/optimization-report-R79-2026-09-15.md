# Optimization report R79 — 2026-09-15

## What the audit found

R76/R77/R78 collapsed three drift surfaces into
`synthia-core` (judge-reply parser, char truncator,
partial redactor). The next grep — `fn encode_cursor`,
`fn decode_cursor` — found a fourth: the **opaque cursor
codec**.

The codec exists in two places:

- `synthia_core::registry::encode_registry_cursor` /
  `decode_registry_cursor` (private, 3 lines each, no
  tests). The doc comment even cross-references the
  server's names: *"The encoding MUST match
  `synthia_server::api::v1::decode_cursor`"*.
- `synthia_server::api::v1::cursor::encode_cursor` /
  `decode_cursor` (public, with **4 dedicated tests**:
  the spec example, ASCII round-trip, unicode round-trip,
  invalid-base64 rejection, non-UTF-8 rejection).

The "MUST match" wording in the core's doc comment is the
exact drift surface R76 found for the judge parser: two
near-duplicates that *must* stay in lockstep, and any
refactor in one would silently desync the other. The
6-test surface lives in one place; the in-memory
registry pagination goes through the other.

The right home is the same `synthia_core` that already
hosts `parse_judge_score` (R76), `truncate_chars` (R77),
and `redact_partial` (R78). A new module
`cursor::encode` / `cursor::decode` is the canonical
implementation. The server's wrappers become 2-line
delegations; the core's private helpers vanish.

## What landed

- New module `crates/synthia-core/src/cursor.rs` —
  `pub fn encode(id: &str) -> String` and
  `pub fn decode(cursor: &str) -> Result<String, Error>`.
  The contract: URL-safe base64, no padding, UTF-8
  validated. Both failure modes (invalid base64, invalid
  UTF-8) collapse to `Error::InvalidItem` because a
  wire-level bad-request is the same regardless of which
  stage failed. The module-doc documents the encoding's
  *stability* guarantee (a cursor emitted by an older
  `synthia-core` round-trips on a newer one and vice
  versa) so a future refactor that wanted to switch
  algorithms is a documented breaking change.
- 6 new tests in `synthia-core` (134 total, was 128):
  `encode_matches_spec_example` (the literal `task_abc →
  dGFza19hYmM` from the doc comment, pinning the
  encoding), ASCII round-trip, unicode round-trip
  (`task_αβγ`), invalid-base64 rejection, non-UTF-8
  payload rejection (single-byte `0xff` is valid
  base64 but not UTF-8), and the `empty → empty`
  round-trip (an empty cursor is the start-of-list
  marker, not an error).
- `synthia-core::lib.rs` re-exports `cursor::{decode,
  encode}` alongside the other primitives; the
  module-doc table gains the `cursor` row.
- `synthia-core::registry::encode_registry_cursor` /
  `decode_registry_cursor` — the private 3-line copies
  that the doc comment warned "MUST match" — are
  reduced to 2-line delegations over the new public
  functions. The same is for the module-level
  `#![allow(clippy::result_large_err)]` and the
  per-function `#[allow(...)]` attribute the core
  uses to acknowledge the 128-byte error size.
- `synthia_server::api::v1::cursor::encode_cursor` /
  `decode_cursor` — 4-line implementations reduced to
  2-line delegations. The 4 server tests
  (`encode_cursor_matches_spec_example`,
  `decode_cursor_round_trip`,
  `decode_cursor_handles_unicode_ids`,
  `decode_cursor_rejects_invalid_base64`,
  `decode_cursor_rejects_non_utf8_payload`) pass
  byte-for-byte against the delegation. The
  non-UTF-8 test still uses `base64` directly to
  build a payload `synthia_core::cursor::encode`
  cannot produce (a non-UTF-8 string), so the test
  itself documents the construction technique.

The wire-level contract (`task_abc` →
`dGFza19hYmM`, the spec example) is now
**documented in `synthia-core::cursor` as a doctest**
and **pinned by a `synthia-core` test**. A future
refactor that changed the encoding would fail both
the doctest and the test, not silently break the
shipped server's API.

## Verification

- `cargo test -p synthia-core --lib` 134 passed (was 128, +6).
- `cargo test -p synthia-server --lib` 400 passed
  (unchanged — the 4 server cursor tests pass
  byte-for-byte against the delegation).
- `make ci` 6/6 green.
- `make test-unit` 2446 passed (was 2440, +6).
- `make examples` exit 0 with `CONSUMER-PROOF: OK` +
  `MVP-OK` + `BEST-OF-N-JUDGE: OK`.
- `make check-mvp-deps` and `make check-public-api-runtime`
  both green: the new module is in `synthia-core`
  (pure string operation, no async, no runtime, no
  HTTP); the server's `use base64::Engine` import
  stays only in the test that needs to build a
  non-UTF-8 payload.

## Deferred

- The server's `cursor.rs` still owns the **page-query
  helpers** (`parse_sort`, `normalize_limit`,
  `resolve_page`, `ParsedSort`, `ResolvedPage`,
  `next_cursor`, `PageQuery`). These are HTTP-shaped
  (they return `synthia_core::Error` for the wire
  surface) and tied to `synthia-server`'s `PageQuery`
  type. They are not the canonical primitive; they
  compose the cursor codec with the rest of the
  page-query shape. Moving them to `synthia-core`
  would pull the HTTP-shaped `PageQuery` struct
  along, which is the wrong direction (the
  primitive belongs in core; the wire envelope
  belongs in the server). Deferred to a follow-up
  that reconsiders whether `PageQuery` should live
  in core too.
