# Optimization report R78 — 2026-09-15

## What the audit found

R77 collapsed the duplicated char-based truncator into
`synthia_core::text::truncate_chars`. The next audit grep
(`fn redact`, `fn mask`, `chars().take(4)`) found a parallel
duplication: the **redaction** primitive.

`synthia_server::api::v1::validation::api_key_mask` is a
19-line function with **6 dedicated unit tests** covering
the partial-redaction contract (first 4 + `***` + last 3,
≤7-char full-mask, multibyte safe, no-leak guarantee). It
is the workspace's only public redactor that preserves a
recognisable prefix while hiding the secret body — the
shape every credential-format log or UI needs. But it lives
inside the server crate, so:

- a future `synthia-provider` debug log of an API key
  would either re-invent the helper or take a server
  dependency (wrong direction);
- a future `synthia-mcp` audit log of a remote tool's
  token has nowhere canonical to redact;
- a future deployment that wants a different
  `keep_first` / `keep_last` (e.g. AWS access keys with a
  4-char prefix and a 4-char suffix) re-implements the
  loop.

The right home is `synthia_core::sensitive` — the same
crate that already hosts `Sensitive` / `SensitiveData`
(the "log this without leaking" newtype), and whose
module-doc explicitly calls itself *"Sensitive
information encryption / redaction primitives"*.

## What landed

- `synthia_core::sensitive::redact_partial(s: &str) -> String` —
  first 4 + `***` + last 3, ≤7-char full-mask fallback,
  multibyte safe, empty input → empty output. The contract
  is fixed at 4+3 because that is the most common API key
  shape (Anthropic `sk-ant-…`, OpenAI `sk-…`, GitHub
  `ghp_…`); the parameterised variant below covers other
  shapes.
- `synthia_core::sensitive::redact_partial_with(s, keep_first,
  keep_last) -> String` — same shape, caller chooses the
  prefix/suffix lengths. Falls back to `"***"` when
  `keep_first + keep_last` would equal or exceed the input
  length (no useful "middle" can be shown).
- 8 new tests in `synthia-core` (128 total, was 120):
  - `redact_partial_long_key_keeps_first_4_and_last_3`
  - `redact_partial_empty_returns_empty`
  - `redact_partial_short_input_is_fully_masked`
  - `redact_partial_exactly_8_chars_shows_4_plus_3`
  - `redact_partial_is_multibyte_safe` — codepoint-count
    contract, not byte-count
  - `redact_partial_does_not_leak_middle_of_long_key`
    — the negative case, pinned
  - `redact_partial_with_respects_keep_first_and_keep_last`
    — the parameterised variant returns the right middle
  - `redact_partial_with_falls_back_to_full_mask_when_input_is_short`
- `synthia_server::api::v1::validation::api_key_mask` — 19
  lines reduced to 2 that delegate to
  `synthia_core::sensitive::redact_partial(key)`. The
  public surface of `synthia-server` is unchanged: the
  same function, same signature, same 6 server tests
  pass byte-for-byte.
- `synthia_core::lib.rs` re-exports `redact_partial` and
  `redact_partial_with` alongside `Sensitive` /
  `SensitiveData`. The module-doc table row for
  `sensitive` now mentions the redaction helper.

## Verification

- `cargo test -p synthia-core --lib` 128 passed (was 120, +8).
- `cargo test -p synthia-server --lib` 400 passed
  (unchanged — the 6 server `mask_*` tests pass
  byte-for-byte against the delegation).
- `make ci` 6/6 green.
- `make test-unit` 2440 passed (was 2432, +8).
- `make examples` exit 0 with `CONSUMER-PROOF: OK` +
  `MVP-OK` + `BEST-OF-N-JUDGE: OK`.
- `make check-mvp-deps` and `make check-public-api-runtime`
  both green: the new helper is in `synthia-core` (pure
  string operation, no async, no runtime).

## Deferred

- The remaining inline `s.chars().take(N).collect()` sites
  (R77's deferred list, plus any new ones found in this
  audit) are still candidates for the same
  `truncate_chars` / `redact_partial` treatment, but each
  is a per-site review.
- `SensitiveData::sanitized` (the "**\***" returner used by
  `Sensitive<T>`'s `Debug` / `Display` / `Serialize` impls)
  is the **other end** of the redaction spectrum: it
  loses all info. The two helpers (`redact_partial` and
  `sanitized`) are complementary, not competing. A
  follow-up R50 could add a `RedactionStyle` enum
  (`Full` / `Partial { keep_first, keep_last }`) so
  `SensitiveData` can opt into the partial style — but
  every existing call site of `sanitized` wants the full
  mask (logs, error contexts, serialization), so the
  default stays `Full`.
