# Optimization report R118 — 2026-09-18

## What the audit found

After R117 (band closed, `routes/chat.rs` being the app layer),
the largest un-split *library* module was
`synthia-context/src/memory/sqlite.rs` — 864 lines holding the
DDL, the error type, the store with its pragmas and recall path,
row decoding, `FTS5` query sanitising, and the `Memory` trait
impl, with a 180-line inline test block.

## What landed

`memory/sqlite.rs` → `memory/sqlite/`:

| file | lines | concern |
|---|---|---|
| `mod.rs` | 68 | module docs + layout + re-exports |
| `schema.rs` | 69 | the idempotent DDL + the schema probe |
| `error.rs` | 59 | `SqliteMemoryError` + the query-error helper |
| `store.rs` | 321 | `SqliteMemory`: constructors, pragmas, `search`, `store_entry`, `writable` |
| `decode.rs` | 57 | raw columns → `MemoryEntry`; the sortable UTC timestamp form |
| `fts.rs` | 21 | `FTS5` sanitising (every term quoted — operator injection impossible) |
| `memory_impl.rs` | 150 | the `Memory` impl (working memory, sessions, delegation) |
| `tests.rs` | 182 | moved out of the inline block |

Visibility: `SCHEMA`, `has_schema`, `query_err`, `read_entry`,
`rfc3339`, `fts_query`, `writable` and the `conn` / `inner` /
`path` / `clock` fields are `pub(super)`; nothing new is `pub`.
`synthia_context::memory::{SqliteMemory, SqliteMemoryError}` and
the facade are unchanged.

## Verification

- `cargo test -p synthia-context --lib` — **95/95**;
  `--features sqlite` — **103/103**.
- `cargo test -p synthia-server` — **402/402** unchanged.
- `cargo clippy --all-targets --all-features --tests --all
  -- -D warnings` — 0 warnings.
- `cargo +nightly fmt --all`; `make ci` **8/8** (band stays 0).

## Result

The optional `SQLite` tier now reads as its docs describe it:
schema in `schema.rs`, the store in `store.rs`, decoding in
`decode.rs`, and the recall-syntax defence in a 21-line
`fts.rs`. Largest file 864 → 321.
