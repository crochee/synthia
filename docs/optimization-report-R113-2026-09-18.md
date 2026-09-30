# Optimization report R113 — 2026-09-18

## What the audit found

After R112, the largest un-split production file in the
workspace was `synthia-server/src/state/app_state.rs` — 1071
lines holding six distinct concerns:

1. the `AppState` struct (~140 lines of heavily documented
   fields),
2. the production constructors (`new` / `with_server_config`,
   ~215 lines) with their config-resolution helpers,
3. the `test-utils` constructor (~110 lines),
4. the per-`(user, session)` controller cache with crash
   repair + `RunDependencies` wiring (~120 lines),
5. the usage counters + API snapshot type,
6. the plugin tool surface + the `AppliedToolSurface` record
   of what `[tools]` did at boot.

R108 had already split the session controller the same way;
`app_state.rs` was the remaining server-side monolith.

## What landed

`state/app_state.rs` → `state/app_state/`:

| file | lines | concern |
|---|---|---|
| `mod.rs` | 270 | struct + accessors (`resolve_agent_name`, `default_user_id`, `max_agents`, `readiness_checks`) + layout table |
| `boot.rs` | 496 | `new` / `with_server_config` + workspace-config loaders + `PerSectionConfig` + `build_default_agent` |
| `for_test.rs` | 134 | the cfg-gated test constructor |
| `sessions.rs` | 134 | controller cache + crash repair + `RunDependencies` wiring |
| `tools.rs` | 76 | `plugin_tool_registry` + `AppliedToolSurface` |
| `usage.rs` | 40 | `UsageMetrics` / `UsageSnapshot` |

Path stability: `state/mod.rs` keeps `pub use
app_state::{AppState, AppliedToolSurface, UsageMetrics,
UsageSnapshot}` (the submodule re-exports back it), and the
`#[cfg(test)] pub(crate) use app_state::plugin_tool_registry`
re-export the `tests/` suite imports is preserved. Zero callers
outside `state/` changed.

`state/mod.rs`'s layout table row for `app_state` updated to
name the submodules.

## Verification

- `cargo test -p synthia-server` — **403/403** across 12 test
  binaries, byte-identical totals to R112.
- `cargo clippy --all-targets --all-features --tests --all
  -- -D warnings` — 0 warnings.
- `cargo +nightly fmt --all` applied; `make ci` **8/8** OK
  (band ratchet stays at 18 — server files carried no inline
  test blocks in the ≥300 tier).
- `cargo build -p synthia-server` (bin + lib) clean.

## Result

The server's state container now reads like the rest of the
post-R104 tree: the struct in `mod.rs`, each lifecycle stage in
its own file. Largest file in `state/` drops 1071 → 496
(`boot.rs`), and the boot's three phases (resolve → build →
assemble) are visible as three blocks in one file instead of
being interleaved with five unrelated concerns.
