# retrospective.md — synthia-interface-contract-closure-cycle-2

## What went well

1. **Atomic commits per fix card** enabled clean per-card review and easy revert
   if needed. Each commit message explains the resolution and references the
   card number.

2. **SSE harness** (`sse-harness.ts`) paid for itself immediately: fix cards
   #003, #004, #008 all used it for Playwright specs and vitest unit tests.
   The protocol-neutral parser (no EventSource polyfill) gave byte-level
   control needed for the half-packet test.

3. **3 fix cards required zero code changes** (#002, #005, #007) because the
   codebase was already compliant — the scanner tests serve as regression
   guards instead.

4. **ARBITRATION.md priorities** consistently resolved ambiguity (SDK types
   win over Synthia spec; camelCase wins over snake_case on the wire).

## What could be improved

1. **Scanner coverage gap**: 31 of 40 endpoints are "backend-only" per
   `contract-check` because the frontend scanner only detects static `fetch()`
   calls in `.ts`/`.tsx` files. Management pages (SettingsPage, ToolsPage,
   SkillsPage, etc.) construct URLs dynamically or use the A2A SDK, which the
   scanner misses. This inflates the "backend-only" count and masks genuine
   gaps.

2. **Playwright coverage low**: Only 7/40 endpoints have dedicated Playwright
   specs. The 33 uncovered endpoints are all management/admin endpoints that
   would benefit from E2E specs in cycle #3.

3. **No live server for Playwright**: `test-contract-closure-playwright` was
   not executed because it requires a running synthia-server. Only vitest
   unit tests were run. CI should provide a server fixture for Playwright
   contract specs.

## Promote-candidates evaluation

### PC-1: Per-normalisation unit tests

**Status: PROMOTE to cycle #3**

Each fix card now has vitest tests verifying its specific contract constraint
(e.g., camelCase fields, enum alignment, lastChunk presence). These should
be promoted to permanent fixtures in CI so regressions are caught early.

**ADR**: Add a `make test-contract-closure` step to CI that runs before
`contract-check`. Currently only `contract-scan` and `contract-coverage`
are in CI; the vitest suite (47 tests) should be added.

### PC-2: Fixture-before-parser

**Status: DEFER**

The SSE harness demonstrates the pattern (test fixtures → parser → spec),
but the broader contract-closure scanner doesn't use fixtures as its source
of truth — it reads the live codebase. Promoting this pattern would require
restructuring the scanner to read from fixture YAML files rather than
scanning source files. This is a larger refactor best done when the scanner
needs to support multi-language backends.

### PC-3: State-machine parsing

**Status: DEFER**

The SSE harness uses an incremental line-based parser that could be
generalised into a state machine for SSE protocol compliance testing.
However, the current line-at-a-time approach is sufficient for Synthia's
SSE needs (no custom extensions). Promote when adding SSE protocol
conformance testing beyond Synthia's own events.

### PC-4: Contract-coverage advisory semantics

**Status: PROMOTE to cycle #3**

The advisory mode (exit 0 even with uncovered paths) has proven its value
during cycle #2: it surfaces gaps without blocking the development flow.
The next step is to evaluate whether to promote to blocking (exit 1) based
on the uncovered path trend.

**ADR**: Track uncovered path count across cycles. If count is 0 for 2+
consecutive cycles, promote to blocking per §6.1. Current count: 33
endpoints (management/admin) + 0 SSE events.

### PC-5: A2A SDK type-checkpoints

**Status: PROMOTE to cycle #3**

Fix cards #002 and #009 both relied on diffing the `@a2a-js/sdk` type
definitions against backend/frontend field names. This should be automated:
a scanner that reads the SDK's TypeScript declarations and cross-references
with contract.yaml.

**ADR**: Add `contract-closure/sdk-type-check.ts` that imports
`@a2a-js/sdk` types and validates that contract.yaml field names match.
Run as part of `make contract-check`.

## §6.1 Decision: CI blocking upgrade

**Decision: NOT YET — defer to cycle #3**

Rationale:
- 33/40 endpoints are uncovered by Playwright specs
- The scanner reports 31 backend-only endpoints due to static analysis limits
- Promoting to blocking now would create noise (false positives from scanner
  gaps) that undermines the purpose of the gate
- Two promote-candidates (PC-1, PC-5) should land first to improve signal quality

**Trigger for cycle #3**: If uncovered path count drops below 10 (driven by
Playwright specs for management pages) AND scanner coverage improves to
reduce false backend-only reports, then promote to blocking.

## Lessons for next cycle

1. **Add vitest to CI** — the 47-unit test suite is the strongest signal we
   have; it should be in the CI pipeline, not just local.

2. **Invest in scanner coverage** — the frontend scanner needs to detect
   dynamic fetch patterns (template literals, SDK method calls) to reduce
   false backend-only reports.

3. **Management page Playwright specs** — the biggest coverage gap is the
   admin pages (Settings, Tools, Skills, Commands, MCP, Memory, Jobs).
   Cycle #3 should add at least smoke tests for each.

4. **Server fixture for CI Playwright** — without a running server, the
   Playwright contract specs can't run in CI. Add a test fixture that starts
   synthia-server with a mock provider.
