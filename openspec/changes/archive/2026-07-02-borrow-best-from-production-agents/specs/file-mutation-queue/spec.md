## ADDED Requirements

### Requirement: File mutation tools SHALL serialize per-filepath via tokio::sync::Mutex

File-mutating tools (`write_file`, `apply_patch`, `edit_file`) MUST acquire a per-filepath mutex before performing any write operation. The mutex map MUST be keyed by the canonicalized realpath (resolving symlinks) of the target file path. The mutex implementation MUST be `tokio::sync::Mutex` (not `std::sync::Mutex`) to remain safe across `.await` points.

#### Scenario: Concurrent writes to same filepath serialize

- **WHEN** two `write_file` tool calls target the same realpath concurrently
- **THEN** the second call blocks until the first completes
- **AND** the file content reflects the second write (no interleaved corruption)
- **AND** both calls return success

#### Scenario: Concurrent writes to different filepaths do not block

- **WHEN** two `write_file` tool calls target different realpaths concurrently
- **THEN** both calls proceed in parallel without blocking
- **AND** both calls return success

#### Scenario: Symlink-targeted write uses realpath as key

- **WHEN** `write_file` is called with path `/tmp/link` where `/tmp/link -> /real/path/file.txt`
- **THEN** the mutex is acquired for realpath `/real/path/file.txt`
- **AND** a concurrent call directly targeting `/real/path/file.txt` blocks on the same mutex

---

### Requirement: File mutation queue map SHALL clean up idle entries

After a file mutation completes and the per-filepath mutex is released, the entry in the mutex map MUST be removed if no other task is waiting on it. This prevents unbounded memory growth across long-running sessions that touch many distinct files.

#### Scenario: Entry removed after single write completes

- **WHEN** a `write_file` call completes and releases the per-filepath mutex
- **AND** no other task is waiting on that mutex
- **THEN** the map entry for that realpath is removed
- **AND** subsequent memory usage does not retain the mutex

#### Scenario: Entry retained while waiter exists

- **WHEN** a `write_file` call completes
- **AND** another task is blocked waiting on the same mutex
- **THEN** the map entry is retained
- **AND** the waiting task acquires the mutex and proceeds
