## ADDED Requirements

### Requirement: Anchored Summary SHALL use 8-section structured template

When the context compaction generates an anchored summary, the output MUST contain exactly 8 sections in the following order: Goal, Constraints, Progress (with Done/InProgress/Blocked subsections), Key Decisions, Next Steps, Critical Context, Relevant Files. Each section MUST be present even if empty (with a placeholder like "_(none)_"). The LLM prompt MUST enforce this structure via explicit template instructions.

#### Scenario: Compaction produces 8-section summary

- **WHEN** context compaction runs and no `previousSummary` exists
- **THEN** the LLM is prompted with the 8-section template
- **AND** the output contains all 8 sections in the specified order
- **AND** missing sections are filled with `_(none)_` placeholder

#### Scenario: Empty Progress section uses placeholder

- **WHEN** the LLM has no in-progress work to record
- **THEN** the Progress.InProgress subsection contains `_(none)_`
- **AND** Progress.Done and Progress.Blocked subsections are still populated if applicable

---

### Requirement: Anchored Summary SHALL support incremental update

When `previousSummary` is available from a prior compaction, the LLM MUST be prompted with "Update the anchored summary" rather than regenerating from scratch. The previous summary MUST be included in the prompt as context. The output MUST preserve the 8-section structure and only modify sections whose content has changed.

#### Scenario: Incremental update preserves unchanged sections

- **WHEN** compaction runs with `previousSummary` containing "Goal: build auth feature"
- **AND** the goal has not changed in the new turn
- **THEN** the output Goal section contains "build auth feature" unchanged
- **AND** only Progress / Next Steps / Critical Context sections are updated

#### Scenario: Incremental update modifies changed sections

- **WHEN** compaction runs with `previousSummary` containing "Progress.InProgress: writing tests"
- **AND** the tests are now complete
- **THEN** the output moves "writing tests" from Progress.InProgress to Progress.Done
- **AND** Progress.InProgress is updated with the new current work (or `_(none)_` if none)

---

### Requirement: Anchored Summary SHALL be token-budget aware

When the previous summary plus the new turn content exceeds the configured token budget, the compaction MUST split the summary at message boundaries (not mid-message) to preserve coherence. If a single message exceeds the budget, mid-message slicing with a marker is permitted as a last resort.

#### Scenario: Summary split at message boundary

- **WHEN** previous summary is 2000 tokens and new turn adds 3000 tokens (budget = 4000)
- **THEN** the compaction includes the full previous summary
- **AND** includes new turn messages up to the budget boundary
- **AND** truncates the remainder at the last complete message boundary

#### Scenario: Mid-message slicing with marker

- **WHEN** a single message is 5000 tokens (budget = 4000)
- **THEN** the compaction slices the message at the 4000-token boundary
- **AND** appends a `[truncated-mid-message]` marker
- **AND** UTF-8 character boundaries are respected (no mid-codepoint slicing)
