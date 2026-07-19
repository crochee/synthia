# agents-md-hierarchical-discovery Specification

## Purpose
Define how Synthia discovers and merges `AGENTS.md` files from `workspace_dir`
upwards to the filesystem root, and how the merged content is injected into
the system prompt. The discovery is hierarchical, the merge order is
"farthest → closest" (so the most specific file can override global
conventions), and the result is cached as `SessionCached` so file edits
between sessions are picked up but LLM-call-to-LLM-call traffic within a
session reuses the read result.

## Requirements

### Requirement: AgentsMdSection SHALL walk ancestors of workspace_dir

The `AgentsMdSection` SHALL, on build, walk `workspace_dir`'s
`Path::ancestors()` from `workspace_dir` upward to the filesystem root, and
collect every existing `AGENTS.md` (or configured filename) found on the way.

#### Scenario: Single AGENTS.md at workspace_dir
- **WHEN** `workspace_dir = /repo` contains `AGENTS.md` and `/repo/..` does
  not contain any `AGENTS.md`
- **THEN** the section builds with the content of `/repo/AGENTS.md`
- **AND** no parent files are included

#### Scenario: Multiple AGENTS.md across ancestors
- **WHEN** `/repo/AGENTS.md` and `/AGENTS.md` both exist and
  `workspace_dir = /repo/sub`
- **THEN** the section builds with `/AGENTS.md` content first followed by
  `/repo/AGENTS.md` content (farthest to closest)

#### Scenario: No AGENTS.md present
- **WHEN** no ancestor of `workspace_dir` contains `AGENTS.md`
- **THEN** the section builds to an empty string
- **AND** `PromptBuilder` SHALL skip the section (no header, no blank line)

#### Scenario: Walks until filesystem root
- **WHEN** `workspace_dir` is several levels deep (e.g. `/a/b/c/d`)
- **AND** `AGENTS.md` exists at `/a` and at `/a/b/c`
- **THEN** the section includes both files
- **AND** the walk terminates at `/` (where `Path::parent()` is `None`)

### Requirement: Merge order SHALL be farthest-to-closest

When multiple `AGENTS.md` files are found, they SHALL be concatenated in
order from the filesystem root toward `workspace_dir` (farthest ancestor
first, closest ancestor last).

#### Scenario: Closer file appears later
- **WHEN** files exist at `/AGENTS.md` and `/repo/AGENTS.md`
- **THEN** the merged content SHALL have `/AGENTS.md` body
- **AND** followed by `/repo/AGENTS.md` body
- **AND** separated by a horizontal rule marker (`---`) and a path header
  for each file

#### Scenario: Header per file
- **WHEN** the section merges two or more files
- **THEN** each file's content SHALL be preceded by a header of the form
  `## AGENTS.md: <absolute path>`
- **AND** the most-specific file's content SHALL appear immediately before
  the closing of the section

### Requirement: AgentsMdSection SHALL cache as SessionCached

The `AgentsMdSection::caching()` SHALL return `SectionCaching::SessionCached`.

#### Scenario: Caching level
- **WHEN** `caching()` is called on `AgentsMdSection::new()`
- **THEN** it SHALL return `SectionCaching::SessionCached`

#### Scenario: Session cache reused across LLM calls
- **WHEN** two consecutive LLM calls are made within the same `PromptState`
  with the same `workspace_dir` and unchanged filesystem
- **THEN** the second call SHALL reuse the first call's section content
  (no second filesystem read)

#### Scenario: Session cache invalidated on session reset
- **WHEN** `PromptState::clear_session()` is called between two LLM calls
- **THEN** the next call SHALL re-walk the filesystem and rebuild the
  section

### Requirement: AgentsMdSection SHALL enforce per-file and total size limits

The section SHALL cap each individual file at 20,000 characters and the
total merged content at 60,000 characters. Exceeding the cap SHALL result
in truncation with an explicit marker.

#### Scenario: Single file under per-file limit
- **WHEN** a single `AGENTS.md` has 1,000 characters
- **THEN** the section includes the file in full
- **AND** no truncation marker is added

#### Scenario: Single file over per-file limit
- **WHEN** a single `AGENTS.md` has 25,000 characters
- **THEN** the section includes only the first 20,000 characters
- **AND** appends a marker of the form
  `[... truncated at 20000 chars - use read for full file ...]`

#### Scenario: Total over aggregate limit
- **WHEN** multiple files together exceed 60,000 characters
- **THEN** the section SHALL stop appending files at the 60,000-character
  boundary
- **AND** append a marker
  `[... total content exceeded 60000 chars; further AGENTS.md files omitted ...]`
- **AND** SHALL include the closest `AGENTS.md` (the most-specific
  override) before any earlier (less-specific) files when both cannot fit

### Requirement: AgentsMdSection SHALL handle missing or unreadable files gracefully

The section SHALL skip files that are missing, are directories, or cannot
be read, and SHALL emit a `tracing::warn!` for each failure. A single
unreadable file SHALL NOT prevent other files from being included.

#### Scenario: Permission denied
- **WHEN** a file exists but `std::fs::read_to_string` returns a permission
  error
- **THEN** the section SHALL emit `tracing::warn!(path, error, ...)`
- **AND** SHALL continue with the remaining files

#### Scenario: File replaced by directory
- **WHEN** `AGENTS.md` is a directory at one ancestor level
- **THEN** the section SHALL skip that entry without warning
- **AND** SHALL continue with the remaining files

#### Scenario: Non-UTF-8 content
- **WHEN** `AGENTS.md` is not valid UTF-8
- **THEN** the section SHALL emit `tracing::warn!`
- **AND** SHALL continue with the remaining files

### Requirement: AgentsMdSection SHALL be configurable

The section SHALL be configurable via `AgentsMdConfig`:
- `enabled: bool` (default `true`)
- `filenames: Vec<String>` (default `["AGENTS.md"]`)
- `max_chars_per_file: usize` (default `20_000`)
- `max_chars_total: usize` (default `60_000`)

#### Scenario: Section disabled
- **WHEN** `AgentsMdConfig::enabled = false`
- **THEN** the section builds to an empty string regardless of
  filesystem contents

#### Scenario: Custom filenames
- **WHEN** `filenames = ["AGENTS.md", "CLAUDE.md"]`
- **THEN** the section walks ancestors looking for either filename
- **AND** includes files matching either name (in order encountered)

#### Scenario: Custom size limits
- **WHEN** `max_chars_per_file = 1_000` and a file is 5,000 characters
- **THEN** the section truncates at 1,000 characters
- **AND** appends the per-file truncation marker reflecting the configured
  limit

### Requirement: IdentitySection SHALL no longer inject AGENTS.md

`IdentitySection::WORKSPACE_FILES` SHALL NOT include `"AGENTS.md"`. The
`IdentitySection` SHALL continue to inject `IDENTITY.md`, `USER.md`, and
`MEMORY.md` from `workspace_dir` only (no ancestor walk).

#### Scenario: WORKSPACE_FILES constant
- **WHEN** `IdentitySection::WORKSPACE_FILES` is read
- **THEN** it SHALL contain exactly `["IDENTITY.md", "USER.md", "MEMORY.md"]`
- **AND** SHALL NOT contain `"AGENTS.md"`

#### Scenario: AGENTS.md at workspace_dir is not in identity
- **WHEN** `workspace_dir/AGENTS.md` exists
- **THEN** `IdentitySection::build` SHALL NOT include that file's content
- **AND** the AgentsMdSection SHALL include it via ancestor walk

### Requirement: PromptBuilder SHALL register AgentsMdSection by default

`PromptBuilder::default_with_sections()` and `PromptBuilder::build_for_name(...)` SHALL include `AgentsMdSection` in the section list, positioned after `EnvironmentSection` and before `MemorySection`.

#### Scenario: Default sections include agents_md
- **WHEN** `PromptBuilder::default_with_sections()` is called
- **THEN** the section list SHALL include `AgentsMdSection`
- **AND** `section_names()` SHALL include `"agents_md"`

#### Scenario: Position in section order
- **WHEN** `PromptBuilder::default_with_sections()` is called
- **THEN** the index of `AgentsMdSection` SHALL be greater than the index
  of `EnvironmentSection`
- **AND** less than the index of `MemorySection`

### Requirement: AgentConfig SHALL expose agents_md configuration

`AgentConfig` SHALL expose two new fields:
- `agents_md_enabled: bool` (default `true`)
- `agents_md_filenames: Vec<String>` (default `["AGENTS.md"]`)

#### Scenario: Field defaults
- **WHEN** `AgentConfig::default()` is called
- **THEN** `agents_md_enabled` SHALL be `true`
- **AND** `agents_md_filenames` SHALL equal `["AGENTS.md"]`

#### Scenario: Field round-trips through serde
- **WHEN** `AgentConfig` is serialized to TOML/JSON and read back
- **THEN** the deserialized `agents_md_enabled` SHALL match the
  serialized value
- **AND** the deserialized `agents_md_filenames` SHALL match the
  serialized value

#### Scenario: Backward compatibility
- **WHEN** an existing `AgentConfig` TOML file (without the new fields) is
  deserialized
- **THEN** `agents_md_enabled` SHALL default to `true`
- **AND** `agents_md_filenames` SHALL default to `["AGENTS.md"]`
- **AND** no deserialization error SHALL occur

### Requirement: Symlink cycles SHALL be detected and stopped

The ancestor walk SHALL canonicalize each candidate path and SHALL skip
any path whose canonical form has been visited in the current walk, to
prevent infinite loops on circular symlinks.

#### Scenario: Circular symlink
- **WHEN** `/repo/loop` is a symlink to `/repo` and `workspace_dir = /repo/loop`
- **THEN** the walk SHALL visit `/repo/loop` and `/repo` (one canonical entry)
- **AND** SHALL stop before re-visiting `/repo/loop`

#### Scenario: Symlink escaping workspace
- **WHEN** a symlink in the ancestor chain points outside the original
  `workspace_dir`'s canonical tree
- **THEN** the walk SHALL continue but SHALL NOT revisit the same canonical
  path twice

### Requirement: Observability hooks SHALL emit AGENTS.md load events

The section SHALL emit `tracing::debug!` events including the absolute path
and the character count of each loaded file, so debug logs reveal which
files were merged.

#### Scenario: Successful load logged
- **WHEN** `/repo/AGENTS.md` is loaded (1,234 chars)
- **THEN** `tracing::debug!` SHALL be emitted with
  `path = "/repo/AGENTS.md", chars = 1234, "agents_md loaded"`

#### Scenario: Failure logged at warn
- **WHEN** a file fails to load
- **THEN** `tracing::warn!` SHALL be emitted with the path and error
- **AND** the section SHALL continue with other files
