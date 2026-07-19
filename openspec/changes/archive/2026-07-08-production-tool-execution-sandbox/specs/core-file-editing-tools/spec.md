## ADDED Requirements

### Requirement: read_file tool SHALL return file contents with optional line range and encoding handling
The system SHALL provide a `read_file` tool that reads a file from the workspace and returns its contents, supporting optional offset/limit and explicit encoding detection.

#### Scenario: Read entire file
- **WHEN** the model invokes `read_file` with a path to a workspace file
- **THEN** the tool SHALL return the full file contents.

#### Scenario: Read line range
- **WHEN** the model invokes `read_file` with `offset=10` and `limit=20`
- **THEN** the tool SHALL return lines 10 through 29 inclusive.

---

### Requirement: write_file tool SHALL atomically replace workspace files
The system SHALL provide a `write_file` tool that writes content to a temporary file and atomically renames it into place, avoiding partial writes.

#### Scenario: Overwrite existing file
- **WHEN** the model invokes `write_file` with an existing workspace path
- **THEN** the tool SHALL first write to a temporary file and then atomically rename it to the target path.

#### Scenario: Write failure mid-operation
- **WHEN** the atomic rename fails due to a filesystem error
- **THEN** the original file SHALL remain unmodified.

---

### Requirement: apply_patch tool SHALL apply structured patches with hunk-level validation
The system SHALL provide an `apply_patch` tool that parses a unified-diff-style patch, validates each hunk against the current file content, and applies validated hunks atomically.

#### Scenario: Apply valid patch
- **WHEN** the model invokes `apply_patch` with a patch whose context lines match the file
- **THEN** the tool SHALL apply all hunks and return the updated file content summary.

#### Scenario: Apply invalid patch
- **WHEN** the model invokes `apply_patch` with a patch whose context lines do not match
- **THEN** the tool SHALL reject the patch, leave the file unchanged, and report which hunks failed validation.

---

### Requirement: apply_patch tool SHALL emit progress events per hunk
The `apply_patch` tool SHALL emit a `FileChangeEvent` for each hunk as it is applied, allowing observers to display incremental progress.

#### Scenario: Multi-hunk patch
- **WHEN** `apply_patch` receives a patch with three hunks
- **THEN** the tool SHALL emit at least three progress events, one after each hunk is applied or rejected.

---

### Requirement: search_files tool SHALL find files by name pattern or content
The system SHALL provide a `search_files` tool that searches the workspace for files matching a glob pattern or containing a literal/regex string.

#### Scenario: Search by glob
- **WHEN** the model invokes `search_files` with pattern `**/*.rs`
- **THEN** the tool SHALL return all Rust source files under the workspace root.

#### Scenario: Search by content
- **WHEN** the model invokes `search_files` with query `TODO` and `regex=false`
- **THEN** the tool SHALL return all files containing the literal string `TODO`.

---

### Requirement: File editing tools SHALL respect workspace boundaries and external directory policy
All file editing tools SHALL refuse to read or write paths outside the configured workspace root unless explicitly approved by the `external_directory` permission.

#### Scenario: Read outside workspace
- **WHEN** the model invokes `read_file` with `/etc/passwd`
- **THEN** the tool SHALL return a permission error without accessing the file.

#### Scenario: External directory approved
- **WHEN** the user has previously approved access to `/tmp/project` via the `external_directory` permission
- **THEN** `read_file` and `write_file` SHALL allow access to paths under `/tmp/project`.

---

### Requirement: File editing tool results SHALL be projected into a model-friendly format
The results of file editing tools SHALL be normalized into a compact, deterministic representation before being appended to the conversation context.

#### Scenario: Large file read
- **WHEN** `read_file` returns a file larger than the configured truncation threshold
- **THEN** the result SHALL be truncated in a UTF-8 safe manner and the full content SHALL be spilled to disk with a reference path.
