# R57 Results — 2026-09-12

Predecessor: [`optimization-report-R56-2026-09-12.md`](optimization-report-R56-2026-09-12.md)
(`2bf3b0d4`).

`AGENTS.md` §4 asks the web frontend for **0 lint, formatting, a
working build, passing tests, and UI tests**. The Rust side has a gate
for every one of its standards (fmt, clippy, rustdoc, dependency
invariants, per-crate tests, examples, benchmark ratios). The frontend
had none of the first three — CI ran the Playwright `e2e` workflow
(server, secrets, browsers) and `rust-quality.yml`, which is Rust-only.

## What the audit found

| Check | Before |
|---|---|
| `npx tsc --noEmit` | clean |
| `npx eslint .` | clean |
| `npx prettier --check .` | **5 files unformatted** — `src/App.tsx`, `src/api/client.ts`, `scripts/e2e/verify_ui.mjs`, `tests/e2e/unit/session-to-messages.spec.ts`, `pnpm-lock.yaml` |
| `npm run build` | clean |
| Who runs these in CI | nobody |

Two of the five are *source* files, and the `Makefile`'s `fmt-web`
formatted `"src/**/*.{ts,tsx,css,md}"` only — so the three others could
never be fixed by the documented command, and the lockfile warning was
permanent (`.prettierignore` had `*.lock`, which does not match
`pnpm-lock.yaml`).

The August report had already noted the lockfile noise
(`docs/optimization-report-2026-08-15.md:432`) and treated it as
cosmetic; it was in fact hiding the four real ones.

## What landed

### A. The four real formatting violations

`npx prettier --write` on the source and test files; `prettier --check .`
is clean. The two `src/` diffs are line-joining only — verified with
`git diff --ignore-all-space` (identical) and by `npm run build`
(tsc + vite) succeeding.

### B. The ignore file matches its own intent

`pnpm-lock.yaml` is listed explicitly, with a comment saying why (`*.lock`
does not match it and lockfiles are generated). Prettier no longer
reports a file nobody should be reformatting.

### C. `make check-web` — the gate, runnable locally

```text
cd synthia-web && npx tsc --noEmit
cd synthia-web && npx eslint .
cd synthia-web && npx prettier --check .
```

`make fmt-web` now formats the whole tree (`prettier --write .`) rather
than `src/**`, so "formatted" means the same thing to `make fmt` and to
`make check-web`.

### D. A `web` job in CI

`.github/workflows/rust-quality.yml` gained a fifth job (`gates`,
`tests`, `examples`, `web`, `bench`): setup-node 20 with the npm cache
keyed on `synthia-web/package-lock.json`, `npm ci`, then `make
check-web`. No browser and no secrets, which is why it belongs in the
quality workflow rather than in `e2e.yml` — that one still owns the
Playwright suites, where a server and credentials are genuinely needed.

## Verification

| Check | Result |
|---|---|
| `make check-web` | tsc clean, eslint clean, `prettier --check .` → "All matched files use Prettier code style!" |
| `npm run build` (tsc + vite) | built in 2.83 s |
| Formatting diff is behaviour-neutral | `git diff --ignore-all-space` on the two `src/` files shows the same hunks as the plain diff (line joining only) |
| Workflow YAML | parses; jobs `gates`, `tests`, `examples`, `web`, `bench` |
| `make ci` | unchanged and green (the web gate is its own job, like `bench`) |

Playwright was **not** re-run: no browser is cached in this workspace
(`~/.cache/ms-playwright` is empty), and downloading chromium is not
something to do opportunistically. The changes are formatting and
gates; `tsc` + `vite build` are the verification that matches them, and
the e2e suite is unchanged. Stated here so the gap is visible rather
than implied.

## Deliberately not done

**Picking one lockfile.** `synthia-web/` tracks both
`package-lock.json` and `pnpm-lock.yaml`; CI installs with `npm ci`
while the August plans and reports drive the frontend with `pnpm`.
Removing either one is a maintainer workflow decision (and the one I
would delete is the one their notes use), so it is recorded in
`docs/README.md`'s known-gaps table instead of acted on.

## Deferred to R58 (recorded, not dropped)

- **A web smoke test with no server**: `vite build` proves compilation,
  not rendering. `scripts/e2e/verify_ui.mjs` exists (and is now
  formatted) — running it in the `web` job would need a browser; the
  trade-off is CI time versus catching a blank page.
- Carried: per-agent tool restriction, the projection's `Arc<Value>`
  schema, `GroupedRegistry`'s residual clones, loop-level benchmark
  ratios, `cargo-deny` in CI, a doctest gate over `SEAMS.md`, an
  in-tree LLM-judge scorer, a per-run strategy override over HTTP (all
  listed with reasons in `docs/README.md`).
