# Optimization report R90 — 2026-09-16

## What the audit found

The user's standing directive ("各create要减少耦合，要高内聚 …
harness 自身也要分好层次和布局") and the standing trigger
"`app_state.rs` 2639 lines" pointed at the same target: the
server-state module. Two facts made this round cheap:

1. The test-block split was already applied across four
   files (R89); `app_state.rs` was the last to receive it.
2. The boot-time helpers in `app_state.rs` are
   `pub(crate)` already; moving them to a sibling module
   does not change any public API.

## What landed

### 1. Test-block split (`crates/synthia-server/src/state/tests/`)

`app_state.rs` carried a 1186-line `#[cfg(test)] mod tests`
block at the bottom. The production file had grown to 2639
lines because of it.

Each test concern now lives in its own focused file under
`tests/`, and a `tests/support.rs` carries the fixtures
shared across more than one submodule (the same shape R89
introduced for `prompt/`, `re_act/`, `registry/`, and
`session/controller/`):

| Submodule | Tests | Concern |
|---|---|---|
| `resolve_agent_name`   | 5 | Three-tier fallback ladder |
| `build_prompt_context` | 7 | Skill discovery + peer-agent snapshot |
| `for_test`             | 1 | `AppState::for_test` wiring |
| `unapplied_keys`       | 2 | R55 inert-key audit |
| `resolve_server`       | 2 | R53 named-config file reader |
| `load_helpers`         | 3 | R16 compaction + R58 restriction |
| `load_strategy`        | 2 | R50 reasoning-loop resolver |
| `boot_surface`         | 4 | R21/R30 MCP + R22 retrieval |
| `apply_tools`          | 3 | R34 `[tools]` application |

Total: 29 tests, byte-identical assertions to the previous
monolithic block (the same R89 discipline: behavioural
tests stay tests, only the file boundaries move).

`tests/support.rs` also drops `OnceLock::get_or_init`
in favour of `std::sync::LazyLock::new` — the stdlib
equivalent, matching the rest of the project.

### 2. Boot helpers extracted (`crates/synthia-server/src/state/boot.rs`)

The 553-line tail of `app_state.rs` — every free function
the boot calls — moved to `boot.rs`:

```
load_default_max_iterations  ─┐
load_default_compaction       │
load_default_tool_restriction │  agents.<name> readers
load_default_strategy         │  (R16 / R50 / R58)
unapplied_agent_keys          │
warn_unapplied_agent_keys    ─┘
build_configured_retriever      R22 retrieval index
register_configured_mcp_servers R21/R30 MCP boot
apply_tools_config              R34 [tools] application
  ├ resolve_group_declarations  (private)
  └ apply_surface_list          (private)
build_prompt_context            prompt assembler input
```

`AppliedToolSurface` (the public record of what `[tools]`
did) **stays in `app_state.rs`** because `AppState` holds
one — splitting a single type across files hurts cohesion
without buying anything. `plugin_tool_registry()` also
stays because `AppState::with_server_config` calls it
and `state/mod.rs` re-exports it.

The new layout (three modules under `state/`):

| Module     | Lines | Responsibility |
|---|---|---|
| `app_state` |  954 | `AppState` + impls, `UsageMetrics`, `AppliedToolSurface`, `plugin_tool_registry` |
| `boot`      |  541 | One free function per boot step |
| `tests`     | 1191 | 9 focused submodules + `support.rs` |

## Why this is the right story

The user's directive was layered: *少冗余代码, 圈复杂度低,
抽象和模块化程度高, harness 自身也要分好层次和布局*. Two
follow-on principles follow from it:

- **One concern per file** — every test concern has its
  own focused module, every boot step has its own focused
  free function. The shared fixtures (`StubAgent`,
  `ScopedHome`, `write_skill_md`) live in exactly one
  place.
- **Visibility tracks audience** — the boot helpers are
  `pub(crate)` and re-exported through `state/mod.rs` only
  for tests. The public API (`AppState`,
  `AppliedToolSurface`, `UsageMetrics`, `UsageSnapshot`)
  is what crosses the crate boundary.

`app_state.rs` is now 954 lines instead of 2639 — a 64%
reduction — and every file in `state/` is small enough to
read end-to-end without scrolling.

## Verification

- `make ci` **7/7 green** (fmt, clippy `-D warnings`,
  rustdoc `-D warnings`, dep invariants, clock ratchet,
  claim language).
- `make test-unit` **2446 passed, 0 failed** across the
  workspace. `synthia-server` lib tests: 403 (unchanged
  from R89, byte-identical assertions).
- `make examples` exit 0; both consumer crates print
  their proof lines (`CONSUMER-PROOF: OK`, `MVP-OK`).
- `cargo build -p synthia-server --all-targets
  --all-features --tests` zero warnings.
- `cargo +nightly fmt --all --check` clean.

## Cognitive complexity

Before R90, `app_state.rs` carried 5 cognitive-complexity
violations (>20 per `clippy.toml`'s 20-threshold):

| Function | Before |
|---|---|
| `load_workspace_config`         | 23 |
| `with_server_config`            | 26 |
| `build_configured_retriever`    | 26 |
| `register_configured_mcp_servers` | 21 |
| `apply_tools_config`            | 25 |

`with_server_config` (26/20) — the boot orchestrator — is
unchanged in complexity this round because the boot helpers
it called are now in a sibling module; the orchestrator's
own complexity did not grow. The other four (each a
single-concern free function) also did not change. R90
moves *code*, not *complexity*: the next round that
addresses complexity will focus on `with_server_config`
itself.

## Deferred

Standing triggers unchanged:

- Reference repos `traitclaw` / `pi` move → re-run the
  R29/R62 parity audit.
- A new drift surface appears → land via the R76–R80
  pattern.
- An architecture gate fails → land the fix.
- A new `#[allow((dead_code|unused))]` slips into library
  code → re-run the R87 grep.

`app_state.rs` itself is still 954 lines. The natural next
target is `with_server_config`'s 26/20 cognitive
complexity: it threads 12 separate steps from "load
config" through "build default agent" through "wire
prompt context" through "install delegation interceptor"
in one function. Splitting the long body into named
phases (each a small private fn the orchestrator calls
in order) drops the orchestrator under threshold without
changing behaviour. R91 candidate.