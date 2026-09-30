# Optimization report R108 — 2026-09-18

## What the audit found

After R107 the workspace's largest production file was
`tool/registry.rs` — 1 369 lines holding the catalog types
(`ToolDescriptor` / `ToolProvenance` / `ToolExposure` /
`ToolCategory` / the snapshot records), the registration value
type (`ToolEntry`) plus its passthrough tool and the internal
storage row (`ProviderEntry`), the `ToolRegistry` struct with
registration/mutation, the version-cached snapshot readers, the
scoped-registration machinery, the `run_stream` dispatcher with
its panic-capturing stream drain, and a private inline
`mod registry_trait` implementing `synthia_core::Registry`.

## What landed

`registry.rs` → `registry/` (one concern per file; the
pre-existing `tests/` subtree keeps its paths):

| File | Lines | Concern |
|---|---|---|
| `mod.rs` | 349 | `ToolRegistry` + `ToolRegistryInner`, registration & mutation, `contains`/`tool_count`/`version`, `Default`/`Clone`, re-exports |
| `catalog.rs` | 110 | descriptor / provenance / exposure / category / snapshot-record types |
| `entry.rs` | 242 | `ToolEntry`, `DynamicPassthroughTool`, `RegistryItem`/`Serialize` impls, `ProviderEntry` + the materialisation fns |
| `scope.rs` | 130 | `RegistrationToken`, `RegistrationScope`, token/session-scope methods |
| `snapshot.rs` | 180 | descriptors / snapshots / provenance, both version-keyed caches |
| `dispatch.rs` | 321 | `run_stream`, the per-tool drain, panic capture, span outcome |
| `registry_trait.rs` | 92 | the `Registry` impl, promoted from an inline `mod` to a real file (body byte-identical to the original inner module) |

Every facade path is unchanged: `synthia_tool::{ToolRegistry,
ToolEntry, ToolDescriptor, ToolExposure, ToolCategory,
ToolMetadataSnapshot, ToolProvenance, RegistrationScope,
RegistrationToken}` re-export from the same `registry` module.
Cross-module seams are `pub(super)`/`pub(crate)`; nothing new is
public. Stale `// 1.` … `// 11.` section-number comments —
navigational scaffolding the split makes redundant — were
removed with the sections they numbered.

Doc links that crossed the new module boundaries
(`[`ToolRegistry::run_stream`]` etc.) were qualified so
`doc-check` stays at zero; that gate caught all six.

## Verification

- `synthia-tool` **215/215**, test-name list byte-identical to
  the pre-split baseline; clippy
  `--all-targets --all-features --tests` **0 warnings**.
- Downstream: `synthia-agent` **247/247**, `synthia-delegation`
  **66/66**, `synthia-server` **403/403**, and the four offline
  tool plugins **14/14/17/17** — all green.
- `make ci` **7/7** (the doc-check failure above was fixed
  before commit); `cargo +nightly fmt --all --check` clean.

## Result

The registry the objective calls the harness's core seam is now
seven single-concern files, none over 350 lines. The largest
production files remaining: `server/routes/chat.rs` (1 180),
`context/memory/file.rs` (1 103 production),
`server/state/app_state.rs` (1 071), `delegation/pool.rs`
(1 036), `session/search.rs` (962).
