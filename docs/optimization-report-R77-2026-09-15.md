# Optimization report R77 — 2026-09-15

## What the audit found

A workspace-wide grep for `s.chars().take(N).collect()` found
**at least 11 call sites** across 8 crates
(`synthia-mcp`, `synthia-session`, `synthia-steering`,
`synthia-rag`, `synthia-attachment`, `synthia-agent`,
`synthia-provider`, `synthia-tool`, plus `synthia-server`).
Each site had its own slight variation:

- inline `let truncated: String = text.chars().take(MAX).collect();
  format!("{truncated}…")` (`synthia-agent::re_act`,
  `synthia-steering::action`)
- a free `truncate_chars` function (`synthia-mcp::naming`)
- an inline truncate-with-excerpt annotation
  (`synthia-provider::error_body`,
  `synthia-provider::json_repair`)
- a 14-line `truncate` function in
  `synthia-server::session::controller` with **6 dedicated
  unit tests** covering char-count vs byte-count, exact-match
  no-ellipsis, multibyte text, and `truncate("x", 0) == "…"`
- a 50-line `truncate_head_tail` / `truncate_in_place` pair
  in `synthia-tool::truncate::bound_output` with its own
  6 tests

The marker formats are inconsistent: `…`, `...`, `[truncated]`,
` [truncated N chars]`, or no marker at all. A future call site
has to either re-invent the helper or copy a 50-line test
suite to keep the contract. The right home is the same
`synthia_core::text` module that already hosts
`cap_to_char_boundary` (the byte-based, in-place sibling).

R77 adds `truncate_chars` — the **char-based**, return-a-pair
variant. The presentation concern (whether to append an
ellipsis, what shape) stays at the call site; the
truncation primitive is one function in one place. The
server's 14-line `truncate` becomes a 6-line wrapper, and
its 6 tests pass unchanged.

## What landed

- `synthia_core::text::truncate_chars(s: &str, max_chars:
  usize) -> (String, bool)` — returns `(s.to_string(), false)`
  when the input is shorter than the cap, else `(first
  max_chars chars, true)`. Char-count, not byte-count. No
  marker; the caller decides.
- 6 new tests in `synthia-core` (120 total, was 114):
  shorter verbatim, exact-match verbatim, one-over
  truncated, empty input, zero max with non-empty (the
  server's corner case), multibyte char-count (中文 / 日本語),
  4-byte emoji.
- `synthia_core::lib.rs` re-exports `truncate_chars` (and
  keeps `cap_to_char_boundary`). The module-doc table row
  is updated to mention both helpers.
- `synthia_server::session::controller::truncate` — 14-line
  function reduced to 6 lines that delegate to
  `synthia_core::text::truncate_chars` and add the `…`
  marker. The 6 dedicated tests for the server's local
  function still pass — the contract is preserved
  byte-for-byte.
- Net: **-8 lines** in the server (14 → 6) and **+1 helper**
  in core (the rest is doc + tests). Future call sites
  that need a char-based truncator have a canonical home
  with 6 tests already pinning the edge cases.

## Why no broader refactor

The audit found 11 inline sites; R77 touches **one** of them
(the server's 14-line function with 6 tests). The other 10
inline `s.chars().take(N).collect()` sites stay untouched on
purpose:

- Their output formats are part of the call site's contract
  (a log preview that ends with `…` looks different from a
  tool result that ends with `[truncated N chars]`). A
  blanket refactor would change observable output, and the
  8 inline sites that produce different markers need
  per-site thought — not a port.
- The server's `truncate` was the one with a 14-line
  implementation and 6 dedicated tests: that **is** a
  primitive, hidden behind a function name, and the
  right thing to do is move it to `synthia-core` and
  make it a primitive. The other 10 sites are one-liners
  that happen to do the same thing; promoting them to
  the canonical helper is a cosmetic change that a
  follow-up round can do per-site when each call site's
  contract is reviewed.
- `synthia-tool::truncate::bound_output` is a different
  concern (head+tail with spill-to-file), and its
  `truncate_in_place` is byte-based like
  `cap_to_char_boundary`. Leaving it alone.

## Verification

- `cargo test -p synthia-core --lib` 120 passed (was 114, +6).
- `cargo test -p synthia-server --lib` 400 passed
  (unchanged — the 6 server `truncate_*` tests still pass,
  pinned to the same `truncate("hello!", 5) == "hello…"`
  shape).
- `make ci` 6/6 green.
- `make test-unit` 2432 passed (was 2426, +6).
- `make examples` exit 0 with `CONSUMER-PROOF: OK` +
  `MVP-OK` + `BEST-OF-N-JUDGE: OK`.
- `make check-mvp-deps` and `make check-public-api-runtime`
  both green: the new helper is in `synthia-core` (pure
  string operation, no async, no runtime).

## Deferred

- 10 inline `s.chars().take(N).collect()` sites across
  `synthia-mcp` / `synthia-session` / `synthia-steering` /
  `synthia-rag` / `synthia-attachment` / `synthia-agent` /
  `synthia-provider` / `synthia-tool` are candidates for
  the same refactor, but each one's output format
  (ellipsis vs annotation vs none) is part of its
  consumer's contract. A per-site review would replace
  the inline `take(N).collect()` with a call to
  `truncate_chars` and preserve the marker shape. Deferred
  to a follow-up R50 — the primitive is now in place.
