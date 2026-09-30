# Optimization report R91 — 2026-09-16

## What the audit found

R90 named `AppState::with_server_config`'s 26/20
cognitive complexity as the R91 candidate. The 287-line
boot orchestrator interleaved three distinct shapes:

1. Eight near-identical per-section `cfg?` chains
   (auth, CORS, operations, default-agent name, max_steps,
   compaction, allowed/denied_tools, strategy).
2. A ~50-line default-agent composition threading
   provider / registry / policy / strategy / restriction
   through the `ReActAgent::with_*` builder chain, plus a
   `try_read()` snapshot and four `if let Some` installers.
3. A 3-arm YAML-bridge match (Ok / Ok(None) / Err) in
   `load_workspace_config`.

Reading the function meant holding all three shapes at
once — the complexity the clippy threshold measures.

## What landed

Three named extractions, each flattening one shape:

### 1. `load_per_section_config` → `PerSectionConfig`

The eight per-section reads moved into one private fn
returning a small struct. Each read follows the same
`cfg? → agents.get(name)? → field → fallback` shape;
grouping them means the reader scans one list and one
shape at a time, not eight interleaved chains. The
orchestrator destructures the struct in one `let` — the
per-section wiring stays visible without the repetition.

### 2. `build_default_agent`

The ~50-line block (registry + steering + the builder
chain + the four optional installers + the `put`) moved
into its own private async fn. It takes the provider,
the registry, the three optional policies, and the
steering bundle; returns the populated `AgentRegistry`.
The steering bundle is built by the orchestrator (the
`AppState` field and the default agent share one `Arc`)
and passed in.

### 3. `bridged_workspace_config` + `legacy_workspace_config`

`load_workspace_config`'s 3-arm match split into:

- `bridged_workspace_config(path)` — one attempt at the
  `--config` YAML, carrying its failure reason as a
  `String` (missing / unsupported / parse failure).
- `legacy_workspace_config(workspace_root)` — the
  `<workspace>/.agents/config.toml` fallback.

The caller is now a flat `match { Ok(cfg), Err(reason) →
log + fallback }`.

## Result

```
$ cargo clippy -p synthia-server -W clippy::cognitive_complexity | grep app_state
(no output — zero violations in app_state.rs)
```

Before R91: `with_server_config` 26/20,
`load_workspace_config` 23/20. After: no function in
`app_state.rs` exceeds the 20 threshold.

`app_state.rs` grew 954 → 1071 lines net (the extracted
fns carry their own doc comments and the two small
structs), but the *orchestrator* — the function a reader
must hold in their head — is now a flat
"resolve → wire → assemble" list of named steps.

## Verification

- `make ci` **7/7 green** (fmt, clippy `-D warnings`,
  rustdoc `-D warnings`, dep invariants, clock ratchet,
  claim language).
- `make test-unit` **2446/2446** across the workspace;
  `synthia-server` 403 (unchanged from R90 — the boot's
  behaviour is identical, only its shape moved).
- `make examples` exit 0; both consumer proofs print
  (`CONSUMER-PROOF: OK`, `MVP-OK`).
- `cargo build -p synthia-server --all-targets
  --all-features --tests` zero warnings.

## Deferred

Standing triggers unchanged (reference repos, drift
surfaces, architecture gates, `#[allow]` regressions).

The next-largest cognitive-complexity violations in the
workspace (from the same clippy run) live in
`synthia-provider` (39/20, 23/20) and
`synthia-context/summarizing.rs` (25/20) — none are in
the server crate. R92 candidate if the user wants the
same treatment extended.