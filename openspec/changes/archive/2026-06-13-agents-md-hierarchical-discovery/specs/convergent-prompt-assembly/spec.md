# convergent-prompt-assembly delta

## ADDED Requirements

### Requirement: IdentitySection workspace-files set SHALL exclude AGENTS.md

`IdentitySection` SHALL inject the workspace-level identity, user, and
memory files from `workspace_dir` only (no ancestor walk). The set of
filenames it injects SHALL be exactly `IDENTITY.md`, `USER.md`, and
`MEMORY.md`. `AGENTS.md` SHALL NOT be in that set, because project-wide
agent instructions are injected by the separate `AgentsMdSection` (see
the `agents-md-hierarchical-discovery` specification).

#### Scenario: WORKSPACE_FILES constant
- **WHEN** `IdentitySection::WORKSPACE_FILES` is read from
  `synthia_context::prompt::sections::identity`
- **THEN** it SHALL contain exactly
  `["IDENTITY.md", "USER.md", "MEMORY.md"]`
- **AND** SHALL NOT contain `"AGENTS.md"`

#### Scenario: AGENTS.md at workspace_dir is not in identity
- **WHEN** `workspace_dir/AGENTS.md` exists
- **THEN** `IdentitySection::build` SHALL NOT include the AGENTS.md
  content
- **AND** the `AgentsMdSection` SHALL include the file via the ancestor
  walk
- **AND** `AGENTS.md` content SHALL appear in the system prompt at the
  position determined by `AgentsMdSection` (after `EnvironmentSection`,
  before `MemorySection`)

#### Scenario: Other three files still injected by identity
- **WHEN** `workspace_dir/IDENTITY.md` exists
- **THEN** `IdentitySection::build` SHALL include it
- **AND** the same SHALL hold for `USER.md` and `MEMORY.md`
