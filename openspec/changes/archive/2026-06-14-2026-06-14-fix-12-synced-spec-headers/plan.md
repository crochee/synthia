# Plan: Fix 12 Synced Spec Headers + Add Format Drift CI Gate

> **Sequence**: Read brainstorm.md → design.md → tasks.md for full context.
> This plan is a tight implementation checklist derived from tasks.md.

## Phase 0: Pre-conditions
- [x] 4-party adversarial review consensus (4-0)
- [x] Socratic problem deconstruction (8 questions answered, 6 design bugs found and fixed)
- [x] `openspec validate 2026-06-14-fix-12-synced-spec-headers` PASS
- [x] Pattern A/B classification corrected (5 + 7)
- [x] spec.md h1→h2 header fix (`# ADDED Requirements` → `## ADDED Requirements`)

## Phase 1: Pattern A 修复（5 specs, 1-line sed each）
For each spec: replace `^## ADDED Requirements$` → `^## Requirements$` (line 7-8 typically)

| # | Spec | Status |
|---|------|--------|
| 1.1 | cache-control-mark | ✓ |
| 1.2 | command-blacklist | ✓ |
| 1.3 | loop-detector-algorithm | ✓ |
| 1.4 | permission-fail-closed | ✓ |
| 1.5 | synthia-session-reexport-policy | ✓ |

## Phase 2: Pattern B 修復（7 specs, prepend `## Purpose` + rename header）
For each spec: prepend a 1-2 paragraph `## Purpose` section before `## Requirements` (extracted from archived proposal.md "Why" section).

| # | Spec | Source archive | Status |
|---|------|----------------|--------|
| 2.1 | context-management | 2026-06-01-synthia-production-ready | ✓ |
| 2.2 | cron-system | 2026-06-01-synthia-production-ready | ✓ |
| 2.3 | error-recovery | 2026-06-01-synthia-production-ready | ✓ |
| 2.4 | memory-system | 2026-06-01-synthia-production-ready | ✓ |
| 2.5 | observability | 2026-06-01-synthia-production-ready | ✓ |
| 2.6 | recovery-cascade-wiring | 2026-06-13-explicit-recovery-paths | ✓ |
| 2.7 | tool-execution | 2026-06-01-synthia-production-ready | ✓ |

## Phase 3: CI Gate Script
- [x] 3.1 Create `scripts/check_synced_spec_format.sh` (~50 lines bash)
- [x] 3.2 Script logic: `grep -rlE '^## (ADDED|MODIFIED) Requirements$' openspec/specs/`
- [x] 3.3 Fail semantics: drift files found → exit 1; clean → exit 0
- [x] 3.4 Self-verification: tested with synthetic drift file (FAIL) and clean state (PASS)
- [x] 3.5 Documentation: top-of-file comment block explains purpose, usage, exit codes, history

**Bug caught during implementation**: Initial `\(ADDED\|MODIFIED\)` syntax was BRE; corrected to ERE `(ADDED|MODIFIED)` for `grep -E`.

## Phase 4: Validation (硬指标)
- [x] 4.1 `openspec spec validate <name> --strict` 12/12 → **12/12 PASS** ✓
- [x] 4.2 `bash scripts/check_synced_spec_format.sh` → **exit 0** ✓
- [x] 4.3 `openspec validate 2026-06-14-fix-12-synced-spec-headers --type change --strict` → **PASS** ✓
- [x] 4.4 `openspec spec validate --strict` 全 61 specs → **无新增 failure** ✓

## Phase 5: OpenSpec 收尾
- [x] 5.1 verify.md (12/12 pass + CI script pass)
- [ ] 5.2 retrospective.md
- [x] 5.3 brainstorm.md (4-party review)
- [ ] 5.4 `openspec archive --skip-specs` (since `openspec/` is gitignored, archive copies worktree files)
- [ ] 5.5 (no git commit needed: `openspec/` is gitignored per project memory)

## Risks Encountered + Mitigated

### R-impl-1: openspec CLI 1.3.1 rejects change names starting with digit
- **Symptom**: `openspec status --change 2026-06-14-...` returns "Change name must start with a letter"
- **Workaround**: `list` and `validate` accept the name; only `status` and `instructions apply` reject it
- **Decision**: Proceed with manual implementation; openspec archive should still work (different code path)
- **Follow-up**: File openspec issue / use `fix-12-synced-spec-headers` alias in future changes

### R-impl-2: grep -E BRE vs ERE syntax mismatch
- **Symptom**: `grep -E '^## \(ADDED\|MODIFIED\) Requirements$'` matches 0 files (literal interpretation of backslashes)
- **Fix**: Use ERE syntax `'^## (ADDED|MODIFIED) Requirements$'`
- **Detection**: Self-test with synthetic drift file revealed 0-drift false positive

## Verification Commands
```bash
# All 12 specs valid
for s in cache-control-mark command-blacklist context-management cron-system error-recovery loop-detector-algorithm memory-system observability permission-fail-closed recovery-cascade-wiring tool-execution synthia-session-reexport-policy; do
  openspec spec validate "$s" --strict --no-interactive
done

# CI script
bash scripts/check_synced_spec_format.sh

# This change
openspec validate 2026-06-14-fix-12-synced-spec-headers --type change --strict --no-interactive
```
