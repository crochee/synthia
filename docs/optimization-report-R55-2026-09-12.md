# R55 Results — 2026-09-12

Predecessor: [`optimization-report-R54-2026-09-12.md`](optimization-report-R54-2026-09-12.md)
(`c3848b34`).

R53 fixed a config *path* that half-applied; R54 fixed knobs nothing
read. One surface was left, and it is the one with a safety edge:
`agents.<name>` parses six keys the boot never applies.

## What the audit found

| Key | Who read it | Consequence of setting it |
|---|---|---|
| `allowed_tools` | nobody | the agent still sees and calls every tool |
| `denied_tools` | nobody | **the agent still calls the tool the operator denied** |
| `hidden` | nobody | the agent is still listed |
| `description` | nobody | the description never reaches the listing the UI reads it from |
| `model` | nobody | the model comes from the descriptor's `model_hint` |
| `color` | nobody | no consumer exists in the UI or the API |

Evidence: none of the six appears outside `AgentConfig`, its tests, and
`config.yaml` — and `config.yaml`, the repo-root sample, uses three of
them (`description`, `hidden`, `color`), so the *intended* shape was
visible while the *implemented* shape stopped at `max_steps` /
`compaction` / `strategy`.

Why this is not merely untidiness: `denied_tools` reads as a security
control. An operator who writes `denied_tools = ["shell"]` believes the
shell is unavailable to that agent. Nothing said otherwise.

## What landed

### A. The audit says so, at boot

```text
WARN synthia.agent: agent config keys are parsed but not applied; what a run
may see and call comes from [tools] (groups / hidden / deferred) and the
registry's privacy flag — see DEPLOYMENT.md
agent=reviewer keys=["description", "model", "allowed_tools", "denied_tools",
"hidden", "color"]
```

One line per configured agent, naming exactly the keys that did nothing,
with the working alternative in the message. The classification is a
pure function (`unapplied_agent_keys`) rather than an inline `if` chain,
so it is testable and there is exactly one place to change when a key
starts being applied.

### B. It stays quiet on a healthy config

`a_clean_agent_config_produces_no_keys` pins that a config using only
`max_steps` / `compaction` / `strategy` produces no warning. A warning
that fires on every deployment is noise, and noise gets filtered — which
would put us back where we started.

### C. `DEPLOYMENT.md` states the contract

A three-row table for what `agents.<name>` applies, the six inert keys
and why, and the two mechanisms that *are* enforced:

- the `[tools]` surface — `hidden` refuses dispatch, not just the
  advertisement (`ToolRegistry::set_hidden`);
- for a library consumer, `synthia_tool::RestrictedRegistry`
  (`ToolRestriction { allow, deny }`), visibility-only by design, which
  is why the deployment-level answer to "may this agent *call* it" is
  `[tools] hidden`.

## Verification

| Check | Result |
|---|---|
| Boot with all six keys set (`/tmp/r55-cfg.yaml`) | the `warn` above, naming the agent and the keys |
| Boot with only `max_steps`/`strategy`/`compaction` | no warning (unit test + the R54 smoke config) |
| `cargo test -p synthia-server` | **431 passed / 0 failed** (429 before: +2 classification tests) |
| Per-crate sweep (19 crates) | **2604 passed / 0 failed**; `synthia-context --features sqlite` 103 |
| `cargo +nightly fmt --all` / `make fmt-check` | clean |
| `cargo clippy --all-targets --all-features --tests --all -- -D warnings` | 0 warnings, 0 errors |
| `make ci` / `make bench-check` | green |

## A decision this round deliberately did not make

`allowed_tools` / `denied_tools` *could* be applied for real, and the
primitive already exists: `ToolRestriction { allow, deny }` +
`RestrictedRegistry::new(inner, restriction)`, which `ToolRestriction`'s
own docs say "may be cheaply passed through the
`AgentBuilder::tool_registry(Arc)` slot". The wiring would be: config →
`ToolRestriction` → wrap the run's registry in the factory (the
controller already resolves the agent name, so it can look the entry up).

It is not done here because it is a **feature**, not a fix, and it is
visibility-only: a restricted tool disappears from the model's tool list
but the registry would still execute it if named, because this framework
deliberately separates "advertised" from "executable" (R33/R34) and the
enforcing knob is `set_hidden`. Shipping half-enforcement under a name
that reads like enforcement is exactly the failure mode R53–R55 exist to
remove, so the honest state is: inert keys are loudly inert, and the
enforced mechanism is documented. A per-agent *visibility* filter plus a
per-agent dispatch deny-list is a coherent feature request, and the
audit's list is the place it would disappear from.

## Deferred to R56 (recorded, not dropped)

- **Per-agent tool restriction** (above), if wanted.
- Carried: the projection's JSON clones (`Arc<Value>` schema, 21.5 µs per
  request — needs `serde/rc`), `GroupedRegistry`'s residual clones,
  ratios for the loop's hot entries (R52), `cargo-deny` in CI (R41), a
  doctest gate over `SEAMS.md` (R47), the builtin tools' runtime seam
  (R40, declined by design), the `Timer` seam (R39), an in-tree
  LLM-judge scorer and a per-run strategy override over HTTP (R50).
