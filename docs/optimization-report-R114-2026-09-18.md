# Optimization report R114 — 2026-09-18

## What the audit found

After R113, the largest lib-crate module never taken through
the one-concern-per-file series was
`synthia-context/src/memory/file.rs` — 1103 lines (545
production + tests in a sibling since R10x) covering nine
concerns: the scope taxonomy, the entry shape, configuration +
scope resolution, the error enum, filesystem defenses, text
helpers, disk scanning + scoring + index rendering, and the
`FileMemory` store + `Memory` impl.

## What landed

`memory/file.rs` → `memory/file/`:

| file | lines | concern |
|---|---|---|
| `mod.rs` | 109 | module docs + layout constants + re-exports |
| `scope.rs` | 84 | `MemoryScope` + directory resolution |
| `entry.rs` | 134 | `MemoryKind` + `FileMemoryEntry` + entry rendering |
| `config.rs` | 101 | `FileMemoryConfig` + `ResolvedScope` |
| `error.rs` | 34 | `FileMemoryError` |
| `guards.rs` | 69 | traversal defenses |
| `text.rs` | 70 | frontmatter / one-line helpers |
| `index.rs` | 220 | scan + score + tokenize + bounded index render |
| `store.rs` | 376 | `FileMemory` + `Memory` impl |

Visibility discipline: every cross-module helper is
`pub(super)` (`ResolvedScope` fields, `scan_dir`, `score`,
`tokenize`, `render_index`, `render_entry`, the guard fns, the
text fns, `MemoryScope::relative`, `FileMemoryEntry::from_memory_entry`,
`ResolvedScope::resolve`). Nothing new is `pub`;
`memory/mod.rs`'s `pub use file::{…}` list is byte-identical.

The module docs' "Defenses" section now names a concrete file
(`guards.rs`) instead of a region of a monolith.

## Verification

- `cargo test -p synthia-context --lib` — **95/95**, test-name
  set unchanged (pure code motion).
- `cargo test -p synthia-context --features sqlite` — **103/103**.
- `cargo clippy --all-targets --all-features --tests --all
  -- -D warnings` — 0 warnings.
- `cargo +nightly fmt --all` applied; `make ci` **8/8**.
- Two intra-doc links fixed after the move (fully qualified).

## Result

The file-memory tier now reads front-to-back as its docs
describe it: scope → entry → config → guards → scan → store.
Largest file 1103 → 376 (`store.rs`); the defenses a reviewer
audits for the "reduce attack surface" objective live in one
69-line file instead of being interleaved with rendering code.
