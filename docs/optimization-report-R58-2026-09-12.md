# R58 Results — 2026-09-12

Predecessor: [`optimization-report-R57-2026-09-12.md`](optimization-report-R57-2026-09-12.md)
(`10cf0e52`).

R55 made the inert `agents.<name>` keys *loud*. Two of them had a safety
edge, and this round makes them work: `allowed_tools` / `denied_tools`.
An operator who wrote `denied_tools = ["shell"]` was protected by
nothing — the agent still called the shell, and nothing in the run said
otherwise.

## The decision R55 deferred, and why it is settled now

R55 declined to wire these keys because `ToolRestriction` +
`RestrictedRegistry` are **visibility-only**: a restricted tool
disappears from the model's tool list, but the registry would still
execute it if named. The framework separates "advertised" from
"executable" on purpose (R33/R34), so shipping a visibility filter under
a name that reads like enforcement would have been exactly the failure
R53–R55 exist to remove.

The way out is to enforce at both ends:

| End | What happens |
|---|---|
| Advertisement | the projection's visible-name set is intersected with the restriction, so the model is never told about an excluded tool |
| Dispatch | a call to an excluded tool is refused **before** hooks and guards, and the model receives an error tool result naming the restriction |

That second end is what makes the key a control. It is also the shape the
framework already uses for a guard denial — error-as-result, a
`WarningKind::Guard` event, the run continuing — so a restriction needs
no new event taxonomy and no new failure mode.

## What landed

### A. The library seam

```rust
let agent = AgentBuilder::new(provider)
    .tool_registry(registry)
    .tool_restriction(ToolRestriction::deny(["shell"]))
    .build();
```

`ReActAgent::with_tool_restriction` (and the builder method) installs it
on the agent; it rides `AgentRuntime.tool_restriction` into the loop,
exactly like `tool_surface`. The two are deliberately different layers,
and both docs say so:

- **`tool_surface`** — the deployment's policy: which groups are active,
  at most how many tools are advertised (R34).
- **`tool_restriction`** — this agent's own scope: what *it* may call.

### B. Both ends, in the loop

`compose_tool_definitions` now intersects the surface's visible names
with the restriction (and `None` + no surface still means "advertise
everything non-hidden", the pre-R33 path — no behaviour change for
deployments that configure neither).

`execute_tool_inner` gained seam 0, before the hook veto and the guard
pipeline:

```text
[denied by tool restriction] `echo` is not available to this agent; use
one of the tools you were given, or answer from what you have.
```

The refusal is a `ToolOutput::error`, so it reaches the model as its tool
result, the tracker records the call, a `WarningKind::Guard` event fires
(the enum's doc now names both sources), and the run completes. A hook
cannot re-enable an excluded tool — policy is checked first.

### C. `task` is not a loophole

Delegation is a *synthetic* tool: it never enters the registry, so a
name-based filter would miss it. The restriction is applied to the
synthetic definition explicitly, which means an allow-list like
`["read", "write"]` also switches delegation off — the honest reading of
"this agent may use these tools". Pinned by a test.

### D. The config hop

`agents.<name>.allowed_tools` / `denied_tools` → `load_default_tool_restriction`
(once, at boot) → `RunDependencies.with_optional_tool_restriction` →
`AgentRunConfig.tool_restriction` → the run factory **and**
`build_react_agent`, so a session run and a registered agent are
restricted the same way — the same two hops `strategy` takes.

Both lists empty → `None`: no restriction object, no extra work, exactly
the pre-R58 path.

## Verification

| Check | Result |
|---|---|
| `cargo test -p synthia-agent` | **328 passed / 0 failed** (325 before: +3) |
| `cargo test -p synthia-server` | **434 passed / 0 failed** (431 before: +3) |
| Behaviour, agent side | `denied_tool_is_hidden_and_its_call_is_refused` — the denied tool is absent from **every** request, the rest of the catalog stays; the call the model makes anyway returns `[denied by tool restriction]` as an error result; a `Guard` warning fires; the session ends `Completed` |
| Behaviour, allow-list | `allow_list_narrows_the_advertised_set` — the advertised set is exactly `["read"]` |
| Delegation | `restriction_without_task_disables_delegation` — `task` is not advertised under an allow-list that omits it |
| Config → wire, end to end | `run_factory_runs_the_configured_strategy` now also builds a restricted run over the same registry: `denied_tools = alpha` on a one-tool registry advertises `[]`, while the unrestricted run advertises `["alpha"]` |
| Config → run config | `run_config_carries_the_dependencies_tool_restriction` |
| Config → restriction | `load_default_tool_restriction_reads_the_agent_table` (deny trims allow), `allow_list_admits_only_its_members` |
| The R55 audit | `unapplied_agent_keys` now reports `["description", "model", "hidden", "color"]` — the pair left the list because it works |
| Full per-crate sweep (19 crates) | **2612 passed / 0 failed** (corrected in R61: the total in this row was hand-summed 10 high; the per-crate numbers above are the sweep output); `synthia-context --features sqlite` 103 |
| `make ci`, `make bench-check`, `make examples` | green (fmt, clippy, rustdoc `-D warnings`, dependency invariants, ratios, examples) |

## Known gaps after this round (in `docs/README.md`)

The per-agent restriction row is gone. Still open, with reasons: the
projection's `Arc<Value>` schema (measured, needs `serde/rc`), the
builtin tools' runtime seam (declined by design), the `Timer` seam,
`GroupedRegistry`'s residual clones, loop-level benchmark ratios,
`cargo-deny` in CI, a doctest gate over `SEAMS.md`, an in-tree
LLM-judge scorer, a per-run strategy override over HTTP, and the two
tracked lockfiles in `synthia-web/`.
