# R54 Results — 2026-09-12

Predecessor: [`optimization-report-R53-2026-09-12.md`](optimization-report-R53-2026-09-12.md)
(`11ef841f`).

R53 fixed one instance of a pattern: *the deployment surface promised
something the code did not do.* R54 finished the audit. The result is a
containerized server that could not be reached through its published
port, a cap that capped nothing, and two config sections that protected
nobody.

## What the audit found

| Finding | Evidence |
|---|---|
| **`SYNTHIA_HOST` / `SYNTHIA_PORT` were read by nothing.** `Dockerfile.server` sets both (`ENV SYNTHIA_HOST=0.0.0.0`), both compose files pass them, `DEPLOYMENT.md` documents them — and the binary's `clap` args had no `env` attribute, only `default_value = "127.0.0.1"` / `"8080"` | `SYNTHIA_HOST=0.0.0.0 SYNTHIA_PORT=18098 synthia-server` logged `Starting … on 127.0.0.1:8080` and bound there |
| **So the documented container was unreachable.** `CMD ["synthia-server"]` passes no flags, and the container binds loopback while Docker publishes the port | `Dockerfile.server:23-36`, `docker-compose{,.prod}.yml` |
| **`host` / `port` in the deployment config were read by nobody** — `ServerConfig` had them, nothing consumed them | R53's audit of the boot loaders |
| **`max_agents` capped nothing.** `POST /api/v1/agents` registered unconditionally; the registry's only bound was memory | `routes/agents.rs::create_agent`, no cap read anywhere |
| **`rate_limit` protected nobody.** `RateLimitConfig { enabled, requests_per_minute, burst }` was parsed and never read — an operator who set `enabled = true` had zero limiting, with no warning | grep: `RateLimitConfig` appears only in its own definition and tests |
| `skills` (a per-deployment skill list) and `model_override` were equally inert; skills are discovered from `<workspace>/.agents/skills/` and the per-agent model comes from `model_hint` | same |

Same disease as R53, three more instances: **silence instead of an
error.** Nothing failed; the knob simply did not exist.

## What landed

### A. The address resolves from every source a deployment has

```rust
pub struct BindAddress { pub host: String, pub port: u16 }

impl BindAddress {
    pub fn resolve(cli: BindOverrides, config: Option<&ServerConfig>) -> Self;
}
```

`--host`/`--port` → `SYNTHIA_HOST`/`SYNTHIA_PORT` (clap's `env` fills
the same `Option` fields the flags do, so the CLI layer stays one
mechanism) → the config's `host`/`port` → the defaults. `Option` rather
than a default-filled value on purpose: a default is indistinguishable
from an explicit `--host 127.0.0.1`, and the config could then never win.
Each part resolves independently, so `--port 7000` plus
`host = "0.0.0.0"` in the config means exactly that.

`boot_server` replaces `create_server` (one caller, both in-crate) and
returns the router **together with** the resolved address, so the log,
the bind, and the announced probe URLs are one value:

```text
INFO synthia_server: binding address=0.0.0.0:18098
INFO synthia_server: listening; probes ready host=0.0.0.0 port=18098 livez="http://0.0.0.0:18098/livez"
```

It also resolves the deployment config **once** and passes it to
`AppState::with_server_config`, extending R53's rule to the one value
that has to exist before any state does — no second read, no second
source of truth.

### B. `max_agents` is a cap

`POST /api/v1/agents` now refuses with the limit and the knob in the
message once the registry holds `max_agents` agents (the built-in
`agent` counts toward it, so the default 5 leaves room for four).
Re-registering an *existing* name stays allowed at the cap: that is a
replace, not growth — and refusing it would break the "update my agent"
flow at exactly the wrong time.

### C. The inert sections are gone

`rate_limit`, `skills`, `model_override`, and with them
`RateLimitConfig` and `SkillConfig` (public config types, so this is a
config-surface break, recorded in the CHANGELOG). `serde` ignores the
keys, so an existing config file keeps parsing and booting.

Why delete rather than implement:

- **Rate limiting** in-process would be *wrong* for a multi-replica
  deployment (each replica would allow the full budget) and is already
  the proxy's job in the shipped topology (`DEPLOYMENT.md`'s Nginx
  section). A dead `enabled` flag is worse than no flag: it reads as
  protection. `DEPLOYMENT.md` now says where limiting belongs.
- **Skills** are discovered from disk by `SkillRegistry` and by the
  `skill` tool; a second, inert list in the config could only confuse
  which one wins.

## Verification

| Check | Result |
|---|---|
| `SYNTHIA_HOST=0.0.0.0 SYNTHIA_PORT=18098` with **no flags** | `binding address=0.0.0.0:18098`, probes announced there (was `127.0.0.1:8080`) |
| `host: "127.0.0.1"`, `port: 18097` in the config with no flags/env | `binding address=127.0.0.1:18097` |
| Flag precedence | `bind_address_resolution_precedence` — CLI > config > defaults |
| Part independence | `bind_address_resolves_host_and_port_independently` — config host + CLI port |
| Env wiring | `the_cli_declares_the_env_vars_the_images_set` — the binary's source carries `env = "SYNTHIA_HOST"` / `"SYNTHIA_PORT"` |
| Registration cap | `test_agent_registration_stops_at_max_agents` — four registrations succeed, the fifth is `400` with `agent limit reached … max_agents` in the message |
| `cargo +nightly fmt --all` / `make fmt-check` | clean |
| `cargo clippy --all-targets --all-features --tests --all -- -D warnings` | 0 warnings, 0 errors |
| `make ci` / `make bench-check` / `make examples` | green |
| Per-crate sweep (19 crates) | **2602 passed / 0 failed** — core 108, telemetry 36, provider 744, context 95 (103 with `sqlite`), tool 311, session 137, steering 74, skill 56, agent 325, attachment 16, mcp 52, rag 38, scheduler 14, macros 31, eval 36, workflow 65, test-support 18, facade 17, server 429 (443 before: −16 tests for the two deleted config types, +2 new) |

The cap test drives the real router (`AppState::for_test` + `create_router`)
rather than the handler, so a regression that moves the check behind the
registration would fail it.

## Deferred to R55 (recorded, not dropped)

- **The remaining `AgentConfig` fields.** `allowed_tools` /
  `denied_tools` / `hidden` / `color` / `description` / `model` in
  `agents.<name>` are still inert: the tools a run may use come from the
  R34 surface policy, the listing comes from the registry descriptors,
  and the model from `model_hint`. Each needs a decision — wire
  (`allowed_tools`/`denied_tools` map naturally onto
  `RestrictedRegistry`, `description`/`color` onto the registered
  descriptor the listing serves) or delete. Deferred as its own round
  because it is a *feature* decision (per-agent restriction) rather than
  an error, and because the frontend's use of `color` needs checking
  first.
- Carried: the projection's JSON clones (`Arc<Value>` schema, 21.5 µs per
  request — needs `serde/rc`), `GroupedRegistry`'s residual clones,
  ratios for the loop's hot entries (R52), `cargo-deny` in CI (R41), a
  doctest gate over `SEAMS.md` (R47), the builtin tools' runtime seam
  (R40, declined by design), the `Timer` seam (R39), an in-tree
  LLM-judge scorer and a per-run strategy override over HTTP (R50).
