# Optimization report R115 — 2026-09-18

## What the audit found

`synthia-session/src/search.rs` (962 lines) was the largest
un-split module in `synthia-session`: the wire types and trait
seam, the JSONL backend (scan / index / sync / remove), the
message-text projection, and the scoring + snippet machinery in
one file, with a 290-line inline test block.

## What landed

`search.rs` → `search/`:

| file | lines | concern |
|---|---|---|
| `mod.rs` | 77 | module docs + `modified_at` + re-exports |
| `query.rs` | 158 | `SearchQuery` / `EntryHit` / `SessionHit` / `SearchError` / `SessionSearch` + `SharedSessionSearch` |
| `backend.rs` | 345 | `JsonlSessionSearch`: scan_store, index_log, sync, the trait impl, `jsonl_session_search` |
| `text.rs` | 64 | searchable text of a folded surface message (tool results excluded) |
| `score.rs` | 80 | term-coverage scoring, whole-word match, snippets |
| `tests.rs` | 289 | the moved test block |

Visibility: `SearchQuery::{effective_limit, terms}` became
`pub(super)` (the backend is their only consumer);
`message_text`, `entry_score`, `snippet` likewise. `IndexedEntry`
/ `SessionIndex` stay private to `backend.rs` — only the backend
touches them. `session`'s crate-root re-export list is
byte-identical.

## Verification

- `cargo test -p synthia-session` — **147/147** lib + 1
  integration, all 10 search test names intact (verified by
  list diff, not just count).
- `cargo test -p synthia-server` — **403/403** unchanged.
- `cargo clippy --all-targets --all-features --tests --all
  -- -D warnings` — 0 warnings.
- `cargo +nightly fmt --all`; `make ci` **8/8** (two intra-doc
  links qualified after the move).

## Result

The search feature now matches its own docs' shape: seam in
`query.rs`, scoring policy in `score.rs` (the "deliberately
simple and documented" formula is one screen), text projection
in `text.rs`, and the JSONL mechanics in `backend.rs`. Largest
file 962 → 345.
