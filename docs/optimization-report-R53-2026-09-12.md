# R53 Results — 2026-09-12

Predecessor: [`optimization-report-R52-2026-09-12.md`](optimization-report-R52-2026-09-12.md)
(`e318ba73`).

A production bug, found while writing R50's config knob and confirmed
here: **`--config <path>` configured only the provider bridge.** Every
other section read at boot came from `<workspace>/config.toml`
regardless of what the operator named — and the repo's own documented
dev entry passes `--config config.yaml`.

## What the audit found

| Finding | Evidence |
|---|---|
| **The named file half-applied.** `AppState::new(workspace_root, config_path)` used `config_path` for exactly one call — `load_workspace_config` (the provider bridge). The ten other boot loaders re-scanned `workspace_root/config.toml` / `.synthia/config.toml` on their own | `crates/synthia-server/src/state/app_state.rs`, `new()` |
| **The documented dev flow hits it.** `make dev` → `dev-server` → `cargo run -p synthia-server -- --config config.yaml`; the repo-root sample config is `config.yaml`, and it contains `agents:` | `Makefile` `dev-server`, `config.yaml` |
| **So these silently did nothing** when named on the command line: `agents.<name>.max_steps` (R15), `agents.<name>.compaction` (R16), `agents.<name>.strategy` (R50), `[tools]` (R34), `auth`, `cors`, `operations` (R29), `retrieval` (R22), `mcp_servers` (R21) | each loader's `for candidate in [workspace_root.join("config.toml"), …]` |
| **And the file was parsed ten times per boot** — once per section, each with its own `exists()` probe and error path | the same helpers |
| The CLI's `--config` help said "Path to an **LLM provider** configuration file", so the behaviour was documented… for the wrong scope: the file is the deployment config the repo ships | `crates/synthia-server/src/main.rs` |

Fail-soft made this invisible: no error, no warning, just settings that
did not apply. The kind of defect that survives a test suite — 2614
tests were green while it was true.

## What landed

### A. One resolution point

```rust
let server_config = resolve_server_config(&workspace_root, config_path);
```

Order: the `--config` path (when it exists) → `<workspace>/config.toml`
→ `<workspace>/.synthia/config.toml`; the format is chosen by extension
(`ServerConfig::load`), so `.toml`, `.yaml`, and `.yml` all work. The
caller keeps `Option<ServerConfig>`: no file means every section falls
back to its default, exactly as before.

A candidate that exists but does not parse is logged and skipped, and
the next is tried. That mirrors the provider bridge's existing contract
(`load_workspace_config` warns and falls back) and means a config typo
degrades to *more defaults*, never to a boot failure.

### B. The sections now read that value

`agents.<name>.max_steps` / `.compaction` / `.strategy`, `[tools]`,
`auth`, `cors`, `operations`, `default_agent`, `retrieval`,
`mcp_servers` — all of them. The five trivial loaders
(`load_auth_config`, `load_cors_config`, `load_operations_config`,
`load_default_agent`, `load_tools_config`) were **deleted**: with the
config already in hand they were one field access each, so the call
site does it directly. The three that carry real logic
(`load_default_max_iterations`, `load_default_compaction`,
`load_default_strategy`) and the two that do work
(`build_configured_retriever`, `register_configured_mcp_servers`) now
take `Option<&ServerConfig>` / `&ServerConfig`.

Net effect beyond the fix: **one read, one parse** per boot instead of
ten, and ~110 lines of duplicated lookup-and-warn gone.

### C. Docs match the behaviour

- `--config`'s help now lists every section it configures and the full
  fallback chain.
- `DEPLOYMENT.md` gained "The deployment configuration file" with the
  precedence order, the three formats, and the fail-soft contract.

## The end-to-end proof

Booted the real binary against a temp workspace with a YAML file the
workspace does not contain:

```bash
synthia-server --directory /tmp/r53-smoke/ws --config /tmp/r53-smoke/cfg.yaml --port 18099
```

```text
INFO synthia_server::state::app_state: loaded provider configuration from --config path=/tmp/r53-smoke/cfg.yaml providers=["openai"]
INFO synthia_server::state::app_state: loaded server config path=/tmp/r53-smoke/cfg.yaml
INFO synthia.agent: configured reasoning strategy agent=agent strategy="best-of-n"
```

`/livez` → `200`, `/api/v1/agents/agent` → the default member. The third
line is the fix: `agents.agent.strategy` came from the named YAML, so
the run factory was given `BestOfNStrategy`. Before R53 the same file
produced only the first line.

Two tests pin the semantics:

- `resolve_server_config_reads_the_named_file` — a named YAML (with
  `max_steps: 5` and `strategy: best-of-n`) wins over a conflicting
  `config.toml` in the workspace, and **without** the flag the same
  workspace resolves the other value (the implicit candidate list is
  deliberately unchanged).
- `resolve_server_config_falls_through_a_broken_candidate` — a
  malformed candidate is skipped, the next is used, and a missing path
  is not an error.

The five deleted loaders' tests were folded into the existing
compaction/strategy/retriever/MCP tests, which now go through
`resolve_server_config` — i.e. they exercise the same path production
does, rather than a helper that production no longer calls.

## Verification

| Check | Result |
|---|---|
| `cargo +nightly fmt --all` / `make fmt-check` | clean |
| `cargo clippy --all-targets --all-features --tests --all -- -D warnings` | 0 warnings, 0 errors |
| `cargo test -p synthia-server` | **443 passed / 0 failed** (441 before: +2 resolver tests) |
| `make test-crates` sweep (19 crates) | **2616 passed / 0 failed** — core 108, telemetry 36, provider 744, context 95 (103 with `sqlite`), tool 311, session 137, steering 74, skill 56, agent 325, attachment 16, mcp 52, rag 38, scheduler 14, macros 31, eval 36, workflow 65, test-support 18, facade 17, server 443 |
| Real-server smoke (above) | `loaded server config` + `configured reasoning strategy … best-of-n`; `/livez` 200 |
| `make ci` | green — fmt-check, clippy, MVP dependency set, runtime-free crates, clock ratchet |
| `make bench-check` | all four performance invariants hold (R52 gate, unchanged) |
| `make examples` | every example plus both consumer crates (`MVP-OK`) |

## Deliberately not done

- **No new implicit candidate (`config.yaml` in the workspace root).**
  It would make a plain `config.yaml` work without `--config`, but it
  would also change which file a deployment reads *without the operator
  asking*, and a stray YAML in a workspace could then reconfigure auth
  or CORS. The documented entry (`make dev-server`, any deployment
  command) passes `--config`, so the fix is complete without that
  surprise. Recorded here so the next person does not read it as an
  oversight.
- **`retrieval.paths` are still resolved relative to the workspace
  root**, not to the config file's directory. That is the documented
  meaning ("paths inside the deployment workspace") and unchanged by
  this round; naming it here because a config file outside the
  workspace makes the question visible.

## Deferred to R54 (recorded, not dropped)

- Carried: the projection's JSON clones (`Arc<Value>` schema, 21.5 µs per
  request — needs `serde/rc` and a wire-type change), `GroupedRegistry`'s
  residual clones, ratios for the loop's own hot entries (R52),
  `cargo-deny` in CI (R41), a doctest gate over `SEAMS.md` (R47), the
  builtin tools' runtime seam (R40, declined by design), the `Timer`
  seam (R39), an in-tree LLM-judge scorer and a per-run strategy
  override over HTTP (R50).
