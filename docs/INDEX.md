# Docs Index — Synthia

This index is the **single entry point** to every document under
`docs/` plus the top-level `*.md` files. It exists so a new contributor
or evaluator can find the right document in two clicks instead of
browsing a flat directory listing.

Read order suggestion for first contact: [`QUICKSTART.md`](QUICKSTART.md)
(5 minutes) → [`MINIMAL.md`](../MINIMAL.md) (the 4-step MVP guide) →
[`ARCHITECTURE.md`](ARCHITECTURE.md) (how the pieces fit) →
[`SEAMS.md`](../SEAMS.md) (every trait a consumer can substitute).

> Looking for **historical** design records, optimization reports,
> and frozen specs? They live in [`ARCHIVE.md`](ARCHIVE.md) and are
> intentionally **not** linked from the live docs; see its header
> for the rationale and the AGENTS §3.7 "claim-language" rule.

---

## 1. Top-level (repo root)

| File | Purpose |
| :--- | :--- |
| [`README.md`](../README.md) | One-page overview, badge strip, scenario table, layer map |
| [`MINIMAL.md`](../MINIMAL.md) | 4-step MVP guide: provider → registry → steering → agent (doctested) |
| [`SEAMS.md`](../SEAMS.md) | The seven substitutable components and the trait that installs yours |
| [`AGENTS.md`](../AGENTS.md) | The contribution contract: env, code rules, gate list, claim-language |
| [`CHANGELOG.md`](../CHANGELOG.md) | Release-by-release record; `[Unreleased]` is the current slice |
| [`CONTRIBUTING.md`](../CONTRIBUTING.md) | How to propose and submit a PR; DCO + Conventional Commits |
| [`CODE_OF_CONDUCT.md`](../CODE_OF_CONDUCT.md) | Community standards (Contributor Covenant v2.1 + CNCF CoC base); report channels |
| [`CLAUDE.md`](../CLAUDE.md) | Anthropic Claude Code standard agent behavior guide (Think / Simplify / …); pairs with AGENTS.md |
| [`GOVERNANCE.md`](../GOVERNANCE.md) | How decisions are made (BDFL → council evolution path) |
| [`MAINTAINERS.md`](../MAINTAINERS.md) | Who can land a PR, and their per-area focus |
| [`SECURITY.md`](../SECURITY.md) | Private vulnerability report, supported versions |
| [`RELEASE.md`](../RELEASE.md) | semver, reproducible artifacts, SLSA Build L3 provenance |
| [`DEPLOYMENT.md`](../DEPLOYMENT.md) | Local dev / docker-compose / Nginx prod / env vars / troubleshooting |
| [`COMMERCIAL.md`](../docs/COMMERCIAL.md) | Production / commercial adoption: SLA, supply-chain, governance |

## 2. Reference (start here)

| File | Purpose |
| :--- | :--- |
| [`docs/QUICKSTART.md`](QUICKSTART.md) | One-dep hello-world (no API key, replays `replay.json`) |
| [`docs/ARCHITECTURE.md`](ARCHITECTURE.md) | Crate-level architecture, dependency direction, runtime neutrality |
| [`docs/DEVELOPING.md`](DEVELOPING.md) | Dev loop, `make` targets, debugging, telemetry |
| [`docs/ROADMAP.md`](ROADMAP.md) | Short- and long-horizon plans; current quarter themes |
| [`docs/SECURITY.md`](SECURITY.md) | Security model, threat surface, audit log |
| [`docs/COMMERCIAL.md`](../docs/COMMERCIAL.md) | Commercial adoption checklist, support tiers |
| [`docs/examples/README.md`](examples/README.md) | Every executable example with proof line, in reading order |

## 3. Design — current

Active design work lives under `docs/superpowers/`:

| Folder | Purpose |
| :--- | :--- |
| [`docs/superpowers/specs/`](superpowers/specs/) | Active specs (a spec is the **current** contract for a change) |
| [`docs/superpowers/decisions/`](superpowers/decisions/) | Cross-cutting decisions (single file, e.g. `2026-08-02-…`) |
| [`docs/superpowers/plans/`](superpowers/plans/) | Implementation plans paired with their spec |
| [`docs/superpowers/specs/<name>/`](superpowers/specs/) | A spec may have a subfolder with `proposal.md` / `tasks.md` / `design.md` |

Naming: spec filenames start with the date the spec was accepted
(`YYYY-MM-DD`), then a slug. Older specs from before 2026-09-19 are
listed under [`ARCHIVE.md`](ARCHIVE.md) — they were **frozen**, not
deleted, to keep git bisect and historical context.

## 5. Architecture decisions (ADR)

[`docs/architecture/adr/`](architecture/adr/) — accepted architectural
decision records, monotonically numbered (0007 onward; the prefix is
preserved by tooling and is **not** renumbered on supersede).

| ADR | Title |
| :--- | :--- |
| [0007](architecture/adr/0007-error-architecture-p2.md) | Error architecture (P2) |
| [0008](architecture/adr/0008-snafu-evaluation.md) | `snafu` evaluation |
| [0009](architecture/adr/0009-opendal-pattern-evaluation.md) | opendal pattern evaluation |
| [0010](architecture/adr/0010-synthia-context-anyhow-strategy.md) | `synthia-context` anyhow strategy |
| [0011](architecture/adr/0011-opendal-rfc-0977-adoption.md) | opendal RFC-0977 adoption |
| [0012](architecture/adr/0012-time-and-id-abstractions.md) | Central `Clock` / `IdGen` abstraction |

## 6. Interface contract

[`docs/interface-contract/`](interface-contract/) — the cross-crate
**wire / surface** contract (schema, arbitration, JSON / YAML / Markdown
forms). Touch this only when changing the `ModelProvider` wire type,
the `Tool` descriptor schema, or the `config.yaml` schema.

| File | Purpose |
| :--- | :--- |
| [`README.md`](interface-contract/README.md) | Index of the contract |
| [`contract.md`](interface-contract/contract.md) | Markdown narrative |
| [`contract.yaml`](interface-contract/contract.yaml) | Machine-readable YAML |
| [`contract.json`](interface-contract/contract.json) | Machine-readable JSON |
| [`SCHEMA.md`](interface-contract/SCHEMA.md) | JSON Schemas per location |
| [`ARBITRATION.md`](interface-contract/ARBITRATION.md) | How contract conflicts are resolved |

## 7. Architecture review

[`docs/architecture/review/`](architecture/review/) — review records
for major refactor phases (`p3-*`). Frozen; they describe the round
as it was.

## 8. Server / routes

| File | Purpose |
| :--- | :--- |
| [`docs/SERVER-ROUTES-TODO.md`](SERVER-ROUTES-TODO.md) | Remaining route work for `synthia-server` (frozen snapshot) |

## 9. Auxiliary — frozen / one-off

| File | Purpose |
| :--- | :--- |
| [`docs/optimization-report-2026-08-15.md`](optimization-report-2026-08-15.md) | Pre-R-number baseline report |
| [`docs/R3_FOLLOW_UP_RESOLVED.md`](R3_FOLLOW_UP_RESOLVED.md) | Resolved follow-ups from the R3 round |
| [`docs/synthia-r5-gap-analysis.md`](../synthia-r5-gap-analysis.md) | R5 gap analysis (closed) |

## 10. Per-crate READMEs

Every workspace crate carries its own `README.md` covering crate-local
usage, features, examples, and known limitations. They are the
**first** thing to read when a PR targets that crate.

| Crate | README |
| :--- | :--- |
| `synthia` | [`crates/synthia/README.md`](../crates/synthia/README.md) |
| `synthia-core` | [`crates/synthia-core/README.md`](../crates/synthia-core/README.md) |
| `synthia-provider` | [`crates/synthia-provider/README.md`](../crates/synthia-provider/README.md) |
| `synthia-context` | [`crates/synthia-context/README.md`](../crates/synthia-context/README.md) |
| `synthia-session` | [`crates/synthia-session/README.md`](../crates/synthia-session/README.md) |
| `synthia-tool` | [`crates/synthia-tool/README.md`](../crates/synthia-tool/README.md) |
| `synthia-harness` | [`crates/synthia-harness/README.md`](../crates/synthia-harness/README.md) |
| `synthia-telemetry` | [`crates/synthia-telemetry/README.md`](../crates/synthia-telemetry/README.md) |
| `synthia-attachment` | [`crates/synthia-attachment/README.md`](../crates/synthia-attachment/README.md) |
| `synthia-mcp` | [`crates/synthia-mcp/README.md`](../crates/synthia-mcp/README.md) |
| `synthia-scheduler` | [`crates/synthia-scheduler/README.md`](../crates/synthia-scheduler/README.md) |
| `synthia-macros` | [`crates/synthia-macros/README.md`](../crates/synthia-macros/README.md) |
| `synthia-eval` | [`crates/synthia-eval/README.md`](../crates/synthia-eval/README.md) |
| `synthia-workflow` | [`crates/synthia-workflow/README.md`](../crates/synthia-workflow/README.md) |
| `synthia-skill` | [`crates/synthia-skill/README.md`](../crates/synthia-skill/README.md) |
| `synthia-steering` | [`crates/synthia-steering/README.md`](../crates/synthia-steering/README.md) |
| `synthia-search` | [`crates/synthia-search/README.md`](../crates/synthia-search/README.md) |
| `synthia-test-support` | [`crates/synthia-test-support/README.md`](../crates/synthia-test-support/README.md) |
| `synthia-tool-read` | [`crates/synthia-tool-read/README.md`](../crates/synthia-tool-read/README.md) |
| `synthia-tool-write` | [`crates/synthia-tool-write/README.md`](../crates/synthia-tool-write/README.md) |
| `synthia-tool-shell` | [`crates/synthia-tool-shell/README.md`](../crates/synthia-tool-shell/README.md) |
| `synthia-tool-todo` | [`crates/synthia-tool-todo/README.md`](../crates/synthia-tool-todo/README.md) |
| `synthia-tool-web` | [`crates/synthia-tool-web/README.md`](../crates/synthia-tool-web/README.md) |
| `synthia-tool-task` | [`crates/synthia-tool-task/README.md`](../crates/synthia-tool-task/README.md) |
| `synthia-tool-scheduler` | [`crates/synthia-tool-scheduler/README.md`](../crates/synthia-tool-scheduler/README.md) |
| `synthia-tool-search` | [`crates/synthia-tool-search/README.md`](../crates/synthia-tool-search/README.md) |
| `synthia-server` | [`crates/synthia-server/README.md`](../crates/synthia-server/README.md) |

## 11. Frontend

[`synthia-web/`](../synthia-web/) — React + Vite single-page client
speaking the REST + SSE chat surface. No e2e harness (per `AGENTS.md`
§4.2): verification is Playwright MCP-driven, not spec-driven.

## 12. Project-owned tooling

| Path | Purpose |
| :--- | :--- |
| [`../contract-closure/`](../contract-closure/) | In-tree TypeScript tool that scans backend router handlers + frontend `fetch`/`api` call sites, cross-checks them, and renders [`docs/interface-contract/`](interface-contract/). CI: `.github/workflows/contract-closure.yml` (currently `continue-on-error: true` — advisory, gates on §6 closure). Local: `make contract-scan / contract-check / contract-report`. |

---

## Where to send what

- **"How do I start?"** → [`QUICKSTART.md`](QUICKSTART.md) →
  [`MINIMAL.md`](../MINIMAL.md)
- **"How do I replace a piece?"** → [`SEAMS.md`](../SEAMS.md)
- **"What just landed / what's next?"** → [`CHANGELOG.md`](../CHANGELOG.md)
  `[Unreleased]` → [`ROADMAP.md`](ROADMAP.md)
- **"Why is it built this way?"** → [`ARCHITECTURE.md`](ARCHITECTURE.md)
  + [`docs/architecture/adr/`](architecture/adr/)
- **"Where is the spec for feature X?"** →
  [`docs/superpowers/specs/`](superpowers/specs/) for active,
  [`ARCHIVE.md`](ARCHIVE.md) for frozen.
- **"I have a question / want to discuss a design"** →
  [GitHub Discussions](https://github.com/crochee/synthia/discussions)
- **"I found a security bug"** → [`SECURITY.md`](../SECURITY.md)
  (private report — never public issue)