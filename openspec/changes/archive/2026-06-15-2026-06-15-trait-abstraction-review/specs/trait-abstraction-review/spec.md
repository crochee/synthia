# trait-abstraction-review Specification

## Purpose

Define the deliverable format and quality criteria for a comprehensive
trait-abstraction review of the 57 `pub trait` definitions in the Synthia
workspace. The review is research-only: it produces an inventory, a
three-bucket classification, and deep reviews for high-signal traits, with
no source-code changes.

## ADDED Requirements

### Requirement: Trait inventory MUST cover all 56 `pub trait` declarations

The review MUST scan every `pub trait` declaration under
`crates/*/src/**/*.rs` and document it in the inventory, totaling 56
declarations (54 in `.rs` files + 2 in `.md` README docs, 51 unique trait
names). Each row MUST include the eight signals defined in design.md §2.

#### Scenario: Trait present with one impl
- **WHEN** the inventory includes a trait that has exactly one `impl`
  block in the workspace
- **THEN** the row's `impl_count` column is `1`
- **AND** the row is classified into KEEP, REVIEW, or REMOVE_CANDIDATE
  per the decision matrix in design.md §3

#### Scenario: Trait absent from inventory
- **WHEN** a `pub trait` exists in `crates/*/src/**/*.rs` or `*.md` but does not
  appear in the inventory
- **THEN** the review is incomplete
- **AND** Phase 5's `KEEP + REVIEW + REMOVE_CANDIDATE` sum MUST equal 56

### Requirement: Decision matrix classification MUST be deterministic

The review MUST classify every trait into exactly one of the three buckets
KEEP, REVIEW, or REMOVE_CANDIDATE by applying the decision matrix in
design.md §3. A trait MUST NOT be left unclassified.

#### Scenario: Singleton trait with low call sites
- **WHEN** `impl_count == 1` AND `call_sites < 3` AND `generic_params == 0`
- **THEN** the trait is classified as REMOVE_CANDIDATE

#### Scenario: Singleton trait with sufficient call sites
- **WHEN** `impl_count == 1` AND `call_sites >= 3`
- **THEN** the trait is classified as REVIEW

#### Scenario: Multi-impl trait with heavy generics
- **WHEN** `impl_count >= 2` AND `generic_params >= 2`
- **THEN** the trait is classified as REVIEW

#### Scenario: Multi-impl trait with low complexity
- **WHEN** `impl_count >= 2` AND `generic_params < 2` AND `call_sites > 0`
- **THEN** the trait is classified as KEEP

### Requirement: Deep review MUST be produced for high-signal traits

The review MUST produce a deep-review file
(`artifacts/deep-reviews/{NN}-{name}.md`) for every trait classified as
REVIEW or REMOVE_CANDIDATE, capped at 15 files. Each deep-review file
MUST include the four sections from design.md §4 (purpose, value,
alternatives, recommendation) and a 4-party adversarial check.

#### Scenario: Deep review file present
- **WHEN** a trait is classified as REVIEW or REMOVE_CANDIDATE
- **THEN** a corresponding deep-review file exists
- **AND** the file includes the 4-party check with at least three parties
  agreeing on the classification

#### Scenario: Classification dispute
- **WHEN** fewer than three parties agree on a classification
- **THEN** the disagreement MUST be recorded in
  `artifacts/disagreements.md`
- **AND** the deep-review file MUST include the dispute in its
  "4-party 检查" section

### Requirement: Future refactor candidates MUST be indexed

The review MUST produce an `artifacts/recommendations.md` file that
includes a "Future refactor candidates" section listing every
REVIEW or REMOVE_CANDIDATE trait with a priority label (P0/P1/P2).
KEEP traits MUST NOT appear in this index.

#### Scenario: Priority labeling
- **WHEN** a trait is REMOVE_CANDIDATE with `impl_count == 1` AND
  `call_sites == 0`
- **THEN** its priority is P0
- **WHEN** a trait is REVIEW with `method_count > 8` OR `generic_params >= 2`
- **THEN** its priority is P1
- **ALL** other REVIEW or REMOVE_CANDIDATE traits are P2

### Requirement: Zero source-code changes

The review MUST NOT modify any file under `crates/*/src/`. All artifacts
MUST be contained within the
`openspec/changes/2026-06-15-trait-abstraction-review/` directory.
The plan file at `docs/superpowers/plans/2026-06-15-trait-abstraction-review.md`
is force-added to git (per project policy for planning docs).

#### Scenario: Source-code pollution check
- **WHEN** `git diff crates/*/src/` is run after applying the change
- **THEN** the diff is empty
- **AND** all new OpenSpec change files are inside the change directory
- **AND** only the plan file (and any updates) appear in git status
