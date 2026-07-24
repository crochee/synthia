## ADDED Requirements

### Requirement: LandlockBackend SHALL detect Landlock ABI availability before selection
The system SHALL probe Landlock ABI support during `LandlockBackend::select` and SHALL return `SandboxAttempt::Unavailable` when Landlock is not supported by the kernel or LSM configuration.

#### Scenario: Linux kernel supports Landlock
- **WHEN** a tool executes on a Linux 5.13+ host with Landlock enabled
- **THEN** `LandlockBackend::select` SHALL return `SandboxAttempt::Landlock` for `Standard` and `Strict` policies.

#### Scenario: Linux kernel does not support Landlock
- **WHEN** a tool executes on a host where Landlock is unavailable
- **THEN** `LandlockBackend::select` SHALL return `SandboxAttempt::Unavailable` without panicking.

---

### Requirement: LandlockBackend SHALL enforce workspace-scoped filesystem access
The Landlock backend SHALL restrict the child process so that it can only access paths inside the configured workspace, plus a fixed set of system directories when `SandboxPolicy::Standard` is active.

#### Scenario: Read workspace file under Standard policy
- **WHEN** a sandboxed command reads a file inside the workspace with `SandboxPolicy::Standard`
- **THEN** the read SHALL succeed.

#### Scenario: Read workspace file under Strict policy
- **WHEN** a sandboxed command reads a file inside the workspace with `SandboxPolicy::Strict`
- **THEN** the read SHALL succeed.

#### Scenario: Read outside workspace under Standard policy
- **WHEN** a sandboxed command attempts to read `/etc/passwd` with `SandboxPolicy::Standard`
- **THEN** the read SHALL fail with a permission error.

#### Scenario: Read outside workspace under Strict policy
- **WHEN** a sandboxed command attempts to read `/etc/passwd` with `SandboxPolicy::Strict`
- **THEN** the read SHALL fail with a permission error.

---

### Requirement: LandlockBackend SHALL map Standard and Strict policies consistently with bubblewrap
The system SHALL apply read-write access to the workspace for both `Standard` and `Strict` policies. Under `Standard` policy, the system SHALL additionally grant read-only access to `/usr`, `/bin`, `/lib`, `/lib64`, `/sbin`, `/proc`, and `/dev`. Under `Strict` policy, the system SHALL deny access to all paths outside the workspace.

#### Scenario: Standard policy grants read-only system access
- **WHEN** a sandboxed command reads `/usr/bin/env` with `SandboxPolicy::Standard`
- **THEN** the read SHALL succeed.

#### Scenario: Strict policy denies system access
- **WHEN** a sandboxed command reads `/usr/bin/env` with `SandboxPolicy::Strict`
- **THEN** the read SHALL fail.

---

### Requirement: Landlock code SHALL be gated by a Cargo feature
The Landlock backend implementation SHALL only compile when the `landlock` Cargo feature is enabled. The default feature set SHALL NOT include `landlock`.

#### Scenario: Default build
- **WHEN** the crate is built without the `landlock` feature
- **THEN** the `landlock` crate dependency SHALL NOT be compiled and `LandlockBackend` SHALL remain a stub that returns `Unavailable`.

#### Scenario: Landlock feature enabled
- **WHEN** the crate is built with `--features landlock`
- **THEN** the real `LandlockBackend` implementation SHALL be compiled.

---

### Requirement: Landlock wrapping SHALL preserve command arguments and environment
When `SandboxAttempt::Landlock::wrap` is applied, the resulting command SHALL execute the original program with the original arguments and environment variables.

#### Scenario: Echo command wrapped with Landlock
- **WHEN** `LandlockAttempt::wrap` is applied to `echo hello`
- **THEN** the executed process SHALL output `hello`.

---
