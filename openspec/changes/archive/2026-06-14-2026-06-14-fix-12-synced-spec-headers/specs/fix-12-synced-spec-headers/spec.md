## ADDED Requirements

### Requirement: All 12 pre-existing format-drifted specs SHALL pass `openspec spec validate --strict`

After this change is applied, all 12 previously failing specs SHALL pass `openspec spec validate <name> --strict` with exit code 0 and SHALL NOT report any validation errors. The 12 specs are: `cache-control-mark`, `command-blacklist`, `context-management`, `cron-system`, `error-recovery`, `loop-detector-algorithm`, `memory-system`, `observability`, `permission-fail-closed`, `recovery-cascade-wiring`, `tool-execution`, and `synthia-session-reexport-policy`.

#### Scenario: All 12 specs pass strict validation
- **WHEN** `for s in cache-control-mark command-blacklist context-management cron-system error-recovery loop-detector-algorithm memory-system observability permission-fail-closed recovery-cascade-wiring tool-execution synthia-session-reexport-policy; do openspec spec validate "$s" --strict --no-interactive; done` is run
- **THEN** every iteration SHALL exit with code 0
- **AND** every iteration SHALL report "is valid" (or equivalent success message)
- **AND** no iteration SHALL report any validation errors

### Requirement: Synced spec files SHALL use `## Requirements` (cumulative format) instead of `## ADDED Requirements` or `## MODIFIED Requirements` (delta format)

After this change is applied, every file under `openspec/specs/*/spec.md` SHALL use the bare `## Requirements` header for its requirements section, NOT `## ADDED Requirements` or `## MODIFIED Requirements`. The 12 fixed specs SHALL follow this rule.

#### Scenario: No ADDED or MODIFIED headers in synced specs
- **WHEN** `grep -l "^## \(ADDED\|MODIFIED\) Requirements" openspec/specs/*/spec.md` is run
- **THEN** the command SHALL return zero matching files
- **AND** the exit code SHALL be non-zero (grep convention: no match = exit 1)

#### Scenario: All synced specs have bare Requirements header
- **WHEN** `grep -l "^## Requirements" openspec/specs/*/spec.md | wc -l` is run
- **THEN** the count SHALL equal the total number of synced specs in `openspec/specs/`

### Requirement: A CI gate script SHALL exist to prevent future format drift

A shell script `scripts/check_synced_spec_format.sh` SHALL exist. When executed, the script SHALL traverse all `openspec/specs/*/spec.md` files, grep for `^## (ADDED|MODIFIED) Requirements`, and exit with non-zero status if any match is found.

#### Scenario: Script exits 0 on clean synced specs
- **WHEN** `bash scripts/check_synced_spec_format.sh` is run in a state where no synced spec contains `## ADDED Requirements` or `## MODIFIED Requirements`
- **THEN** the script SHALL exit with code 0
- **AND** SHALL print a success message indicating the synced specs are in cumulative format

#### Scenario: Script exits 1 on format drift
- **WHEN** a test file containing `## ADDED Requirements` is temporarily placed in `openspec/specs/test-drift/spec.md`
- **AND** `bash scripts/check_synced_spec_format.sh` is run
- **THEN** the script SHALL exit with code 1
- **AND** SHALL print the offending file path
- **AND** SHALL be reverted (the test file removed) after verification

#### Scenario: Script is self-documenting
- **WHEN** `head -10 scripts/check_synced_spec_format.sh` is read
- **THEN** the script SHALL contain a comment block explaining its purpose
- **AND** SHALL reference the OpenSpec cumulative-format rule

### Requirement: This change SHALL NOT modify any source code outside spec files and the CI gate script

This change is a maintenance task. It SHALL modify ONLY:
- 12 files under `openspec/specs/*/spec.md`
- The new file `scripts/check_synced_spec_format.sh`
- This change's own OpenSpec artifacts under `openspec/changes/2026-06-14-fix-12-synced-spec-headers/`

It SHALL NOT modify any file under `crates/`, `tools/`, `tests/`, or any other source directory.

#### Scenario: No source code changes
- **WHEN** `git diff --stat` is run with a filter for `crates/`, `tools/`, `tests/`
- **THEN** the output SHALL be empty
- **AND** the only changed files SHALL be the 12 spec files + 1 new script + 8 OpenSpec artifacts (proposal.md, design.md, tasks.md, plan.md, brainstorm.md, verify.md, retrospective.md, specs/.../spec.md)

#### Scenario: No new dependencies
- **WHEN** `git diff --stat -- '**/Cargo.toml'` is run
- **THEN** no Cargo.toml SHALL be modified
- **AND** no new dependencies SHALL be added

### Requirement: Pattern B specs SHALL include a `## Purpose` section before `## Requirements`

The 7 Pattern B specs (`context-management`, `cron-system`, `error-recovery`, `memory-system`, `observability`, `recovery-cascade-wiring`, `tool-execution`) currently lack a `## Purpose` section. After this change, each of these 7 specs SHALL include a `## Purpose` section (one or more paragraphs describing the spec's purpose) immediately after the spec title and BEFORE the `## Requirements` section.

#### Scenario: All 7 Pattern B specs have Purpose section
- **WHEN** `for s in context-management cron-system error-recovery memory-system observability recovery-cascade-wiring tool-execution; do head -10 openspec/specs/$s/spec.md; done` is run
- **THEN** each iteration SHALL show a `## Purpose` section before `## Requirements`
- **AND** each `## Purpose` section SHALL contain at least one paragraph of text

#### Scenario: Purpose text recovered from archived change proposal
- **WHEN** the `## Purpose` text of any Pattern B spec is read
- **THEN** the text SHALL be sourced from the corresponding archived change's `proposal.md` "Why" section
- **AND** SHALL be marked as such (e.g., comment "(recovered from <archive-path>)") if it is reconstructed

### Requirement: This change SHALL pass openspec validation

After all OpenSpec artifacts are created, `openspec validate 2026-06-14-fix-12-synced-spec-headers --type change --strict` SHALL exit with code 0 and SHALL NOT report any validation errors.

#### Scenario: Strict validation passes
- **WHEN** `openspec validate 2026-06-14-fix-12-synced-spec-headers --type change --strict --no-interactive` is executed
- **THEN** the exit code SHALL be 0
- **AND** the output SHALL contain the text "Change '2026-06-14-fix-12-synced-spec-headers' is valid"
- **AND** no validation errors or warnings SHALL be reported

#### Scenario: All ADDED Requirements have at least one Scenario
- **WHEN** `openspec/changes/2026-06-14-fix-12-synced-spec-headers/specs/fix-12-synced-spec-headers/spec.md` is read
- **THEN** every `### Requirement:` heading SHALL be followed by at least one `#### Scenario:` heading
- **AND** no Requirement SHALL exist without an associated Scenario

#### Scenario: First sentence contains SHALL or MUST
- **WHEN** `openspec/changes/2026-06-14-fix-12-synced-spec-headers/specs/fix-12-synced-spec-headers/spec.md` is read
- **THEN** the first sentence of every Requirement's body (text before first period/newline after heading) SHALL contain either "SHALL" or "MUST"
