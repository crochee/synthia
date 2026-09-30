# Optimization report R109 — 2026-09-18

## What the audit found

R105–R108 hand-swept the worst inline test blocks and the two
largest production files, but the discipline had no enforcement:
nothing stopped the next 400-line test module from landing inline
again. Two files still sat right at the ≥400 line —
`provider/types/content.rs` (401-line block) and
`provider/openai/types.rs` (400).

## What landed

**The ≥400 tier is closed.** Both blocks moved to sibling
`content/tests.rs` (398) and `openai/types.rs → openai/tests.rs`
(397); production untouched. No production file workspace-wide
holds an inline `#[cfg(test)] mod` block of 400+ lines.

**The bar is now a ratchet**, mirroring `check-clock`'s design:

- `make check-test-layout` — two rules: a **hard limit**
  (`TEST_BLOCK_LIMIT := 400` — no inline test module may reach
  400 lines) and a **shrinking band** (`TEST_BLOCK_BASELINE := 20`
  — the count of ≥300-line blocks may only decrease). Wired into
  `make ci` as its eighth gate.
- `make test-layout-audit` — lists every ≥300-line inline block
  (currently 20, top: `openai_streaming/processor.rs` 386,
  `provider/config.rs` 383, `session/surface.rs` 373).
- Scanner mirrors `check-clock`'s per-file awk: it counts a block
  only where `#[cfg(test)]` is followed by `mod X {` (cfg-gated
  imports like `worktree.rs`'s don't count), measuring to EOF.

Both failure paths were negative-tested:
`make check-test-layout TEST_BLOCK_LIMIT=100` fails with a full
listing; `TEST_BLOCK_BASELINE=19` fails on the band rule.

AGENTS.md §3.6 documents both targets and the widened `make ci`
row. Production-file splits (chat.rs 1 180, memory/file.rs 1 103,
app_state.rs 1 071, pool.rs 1 036) stay judgment rounds — a size
ratchet on genuinely single-concern files would be a blunt proxy.

## Verification

- `synthia-provider` **690/690**, test-name list byte-identical.
- Airtight close for R108's comment-only tail:
  `synthia-agent` **247/247**, `synthia-tool` **215/215** re-run
  post-cleanup.
- `make ci` **8/8 OK**, exit 0 — the new gate's line reads
  `OK: 20 inline test module(s) >=300 lines (baseline 20); none
  at or above 400`.
- `make -n ci` parses (Makefile syntax); `make check-clock`
  unchanged; `cargo +nightly fmt --all` applied.

## Result

The layout invariant R105 established is now machine-enforced:
the workspace cannot regress past the bar, and each future round
that extracts a band member lowers `TEST_BLOCK_BASELINE` by one.
Next candidates by size are listed by `make test-layout-audit`
itself.
