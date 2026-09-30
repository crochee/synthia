# Adopting Synthia Commercially

A self-contained checklist for teams evaluating Synthia for
production / paid / regulated workloads. Every line is backed by a
file in this repository — none of it is marketing copy.

## 1. Licensing

| Concern | Answer |
|---|---|
| License | MIT ([LICENSE](../LICENSE)) |
| Patent grant | MIT does not grant patents explicitly; check your counsel's risk model |
| Dual-license path for paid use | Not offered today. Fork under MIT and ship as your own derivative. |
| Copyright holder | `synthia contributors` (see [LICENSE](../LICENSE)); per-copyright authorship preserved by `git log --follow <file>` |
| NOTICE obligations | None (MIT only) |

If you need Apache-2.0 (for the explicit patent grant), see [docs/COMMERCIAL.md §History] below; this repo is on MIT as of 2026-09-30 (R124).

## 2. Security & vulnerability handling

| Concern | Where to look |
|---|---|
| How to report a private vuln | [SECURITY.md §报告漏洞](../SECURITY.md) |
| Disclosure SLA | 72h acknowledgement, 90d coordinated disclosure |
| Supported versions | Only the latest release (rolling); `Cargo.toml` version is the single source of truth |
| Threat model (in scope / out of scope) | [SECURITY.md §威胁模型](../SECURITY.md) |
| How the project handles a CVE | Public advisory on GitHub + fixed release + release-notes credit (unless anonymity requested) |

## 3. Supply chain

| Concern | Where to look |
|---|---|
| Locked dependencies | `Cargo.lock` checked in (AGENTS.md §3.1) |
| License / ban / source audit | [`deny.toml`](../deny.toml) + `make check-deny` |
| Known-vuln scanning | `cargo audit` weekly + per-PR ([security-audit.yml](../.github/workflows/security-audit.yml)) |
| Secret scan | `secret-scan` job in security-audit.yml |
| Release artifacts | linux x86_64 / windows x86_64 / macOS arm64 + `sha256sums.txt` ([release.yml](../.github/workflows/release.yml)) |
| SBOM | CycloneDX `dist/sbom.json` ([release.yml](../.github/workflows/release.yml)) |
| Build provenance | SLSA Build L3 via `actions/attest-build-provenance` ([release.yml](../.github/workflows/release.yml)) |
| Source signing | Signed tags (`git tag -s …`, RELEASE.md §发布步骤 §3) |

## 4. Quality gates (every PR)

| Gate | Tool | Where |
|---|---|---|
| Formatting | `cargo +nightly fmt --all -- --check` | [`Makefile`](../Makefile) `fmt-check` |
| Lint | `cargo clippy -D warnings --all-targets --all-features --tests --all` | [`Makefile`](../Makefile) `lint-rust` |
| Doc | rustdoc with `-D warnings` | [`Makefile`](../Makefile) `doc-check` |
| Tests | per-crate `cargo test -p <crate>` | [`Makefile`](../Makefile) `test-crates` |
| Examples | `cargo run --example …` for every example + both standalone consumer crates | [`Makefile`](../Makefile) `examples` |
| Bench invariants | ratio gate (memoised path vs cold path) | [`Makefile`](../Makefile) `bench-check` |
| Frontend gates | `tsc --noEmit` + `eslint` + `prettier --check` | [`Makefile`](../Makefile) `check-web` |
| Dependency shape | MVP subset pulls no HTTP / OTel / DB | [`Makefile`](../Makefile) `check-mvp-deps` |
| Runtime shape | runtime-free crates pull no tokio | [`Makefile`](../Makefile) `check-no-runtime` |
| Public API shape | lib public APIs name no runtime type | [`Makefile`](../Makefile) `check-public-api-runtime` |
| Harness shape | functions ≤ 100 lines, nesting ≤ 4 | [`Makefile`](../Makefile) `check-harness-shape` |
| Wall clock | `chrono::Utc::now()` forbidden in lib production code | [`Makefile`](../Makefile) `check-clock` |
| Pub surface | `pub use <非 synthia_*>::*;` forbidden | [`Makefile`](../Makefile) `check-pub-surface` |
| Claim language | "paradigm only" / "default set" forbidden in current-state docs | [`Makefile`](../Makefile) `check-claim-language` |

Reproduce locally with one command:

```bash
make ci
```

## 5. Architecture properties relevant to commercial adoption

| Property | Verification |
|---|---|
| One dependency, many features | `cargo add synthia`; default features compile a basic agent; `make check-mvp-deps` proves the seven-feature subset compiles no HTTP / OTel / DB |
| Runtime choice | `synthia_core::spawn::Spawner` is the one seam; `cargo run --example runtime_agnostic -p synthia-harness` proves a full turn on `std::thread + futures::executor` |
| Time is injectable | `synthia_core::Clock`; `make check-clock` forbids `chrono::Utc::now()` in lib production code |
| IDs are injectable | `synthia_core::IdGen` (ULID default) |
| Public-API runtime leakage | none (gated by `make check-public-api-runtime`) |
| Contract closure (frontend ↔ backend) | scanned on every PR ([contract-closure.yml](../.github/workflows/contract-closure.yml), advisory until §6 of the closure spec; `[Unreleased]` of [CHANGELOG.md](../CHANGELOG.md) tracks the gating flip) |
| Plugin isolation | tool plugins are separate crates; each plugin owns its boundary policy (e.g., `ShellTool` owns `ExecutionPolicy` + `SandboxBackend`) |

## 6. Decision records

The project's design history is recorded as round-numbered
"optimization reports" — every adoption cites the reference (pi-mono
/ DeepSeek DSH / traitclaw / pi-subagents) it came from and what
declined, so a commercial adopter can audit how each decision was
made:

- [`docs/ROADMAP.md`](ROADMAP.md) — current trajectory; explicitly
  names pi-mono / DeepSeek DSH as the community references.
- [`docs/reference-parity.md`](reference-parity.md) — one row per
  capability mapped to a Synthia counterpart or a "declined with
  reason" entry.
- [`docs/optimization-report-R*.md`](optimization-report-R10-2026-09-11.md)
  — per-round records (96 files, e.g. R10 above). Frozen by R46
  precedent (current-state claim-language gate excludes them).
  Catalogued in [`docs/ARCHIVE.md`](../docs/ARCHIVE.md) §2.

## 7. Commercial support

There is no paid support tier today. Commercial inquiries may
contact the maintainer via the repository's GitHub profile; see
[SUPPORT.md](../.github/SUPPORT.md) for the official channel set.

The project is best-effort, volunteer-maintained today. A team that
needs SLA-bound support should budget for either:

1. in-house ownership of a fork under MIT, with a Synthia-aware
   staff engineer on call; or
2. a sponsored-maintainer arrangement negotiated with the
   maintainer.

## 8. Compliance checklist (copy/paste for your review board)

```text
[ ] License reviewed (MIT, see LICENSE) — counsel approved.
[ ] Single dependency point of contact is the synthia facade
    (verified: `synthia-server/Cargo.toml` has only one synthia
    line; facade forwards everything else; AGENTS.md §1 R121 rule).
[ ] SBOM (CycloneDX) is generated on every release and archived
    by our artifact store.
[ ] Release binaries are signed (SLSA Build L3 provenance +
    cosign keyless) and verified by our artifact store before
    promotion.
[ ] Vulnerability SLA acknowledged: 72h acknowledgement + 90d
    coordinated disclosure. We subscribe to GitHub Security
    Advisories for this repo.
[ ] CVE policy: we mirror the project's advisory channel into our
    internal ticketing system.
[ ] Release cadence: project follows semver; we pin to a minor
    and roll forward monthly.
[ ] Fork plan: documented (where to fork, how to rebase, who
    owns the fork's release engineering).
```

## History

- 2026-09-30 (R124): dropped dual `MIT OR Apache-2.0` and aligned all
  27 crates on `MIT` to simplify the corporate license review path.
  See `docs/optimization-report-R*.md` (the round report at the matching timestamp) for the audit.
