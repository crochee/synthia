## ADDED Requirements

### Requirement: SandboxManager SHALL provide a cross-platform abstraction for sandbox selection
The system SHALL define a `SandboxManager` trait with a `select` method that returns a `SandboxAttempt` based on a `SandboxPolicy` and the current operating system.

#### Scenario: Linux sandbox selection
- **WHEN** a tool executes on Linux with sandboxing enabled
- **THEN** `SandboxManager::select` SHALL return a Linux-specific sandbox attempt, preferring `Bubblewrap` as the default backend.

#### Scenario: Unsupported platform
- **WHEN** a tool executes on a platform with no implemented backend
- **THEN** `SandboxManager::select` SHALL return `SandboxAttempt::Unavailable` and the caller SHALL apply the configured unavailable policy.

---

### Requirement: Linux bubblewrap backend SHALL restrict filesystem access to the workspace
The `Bubblewrap` sandbox SHALL mount the workspace root as read-write and SHALL deny access to all paths outside the workspace, except for explicitly allowed system directories required by the shell.

#### Scenario: Read workspace file
- **WHEN** a sandboxed `bash` command reads a file inside the workspace
- **THEN** the read SHALL succeed.

#### Scenario: Read outside workspace
- **WHEN** a sandboxed `bash` command attempts to read `/etc/passwd`
- **THEN** the read SHALL fail with a permission error.

---

### Requirement: Sandbox policy SHALL be configurable per session and per tool type
The system SHALL allow `SandboxPolicy` to be configured globally, per session, and overridden per tool type, with an explicit fallback chain.

#### Scenario: Session-level sandbox off
- **WHEN** a session configuration sets `sandbox_policy = SandboxPolicy::None`
- **THEN** `SandboxManager::select` SHALL return `SandboxAttempt::None` for all tools in that session.

#### Scenario: Tool-level override
- **WHEN** the `bash` tool is configured with `SandboxPolicy::Strict` while the session default is `SandboxPolicy::Standard`
- **THEN** bash invocations SHALL use the `Strict` policy.

---

### Requirement: Sandbox unavailability SHALL default to deny unless explicitly configured otherwise
When the selected sandbox backend is unavailable, the default behavior SHALL be `Deny`. An optional `OnUnavailable::Prompt` setting MAY allow the user to explicitly approve un-sandboxed execution.

#### Scenario: bubblewrap binary missing
- **WHEN** the `bwrap` binary is not found on Linux
- **THEN** `SandboxManager` SHALL report unavailable and `ToolOrchestrator` SHALL deny the invocation by default.

---

### Requirement: Sandbox execution SHALL support timeout and cancellation
The sandbox wrapper SHALL propagate timeout and cancellation signals to the underlying process and report the reason accurately.

#### Scenario: Command timeout inside sandbox
- **WHEN** a sandboxed command exceeds its configured timeout
- **THEN** the sandbox runner SHALL terminate the process and return a timeout error.

---

### Requirement: Landlock and seccomp backends SHALL be available as optional features
The system SHALL expose `landlock` and `seccomp` sandbox backends behind Cargo features, allowing additional hardening on supported kernels.

#### Scenario: Landlock feature enabled
- **WHEN** the `landlock` feature is enabled on a Linux 5.13+ kernel
- **THEN** `SandboxManager` MAY select the Landlock backend for finer-grained filesystem access control.
