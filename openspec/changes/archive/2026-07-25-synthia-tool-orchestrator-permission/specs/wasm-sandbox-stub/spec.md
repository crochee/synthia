# Spec: wasm-sandbox-stub

## ADDED Requirements

### Requirement: SandboxAttempt::Wasm variant

`SandboxAttempt` SHALL have a `Wasm { runtime: String }` variant.

#### Scenario: Wasm variant exists in match arms

WHEN code matches on `SandboxAttempt`
THEN the `Wasm { runtime }` arm SHALL be available
AND SHALL return `ToolOutput::error("WASM sandbox not yet implemented: runtime={runtime}")`

#### Scenario: Wasm variant serialized

WHEN `SandboxAttempt::Wasm { runtime: "wasmtime".into() }` is serialized to JSON
THEN the result SHALL be `{"Wasm":{"runtime":"wasmtime"}}`

### Requirement: Plugin tools SHALL require Wasm sandbox (future)

Plugin tools SHALL be executed in a WASM sandbox when the runtime is available. Until then, `SandboxAttempt::Wasm` SHALL return a clear error indicating the sandbox is unsupported.

WHEN a tool has `ToolProvenance::Plugin` AND `SandboxAttempt::Wasm` is specified
AND the WASM runtime is not yet implemented
THEN the tool execution SHALL fail with a clear error message

#### Scenario: Plugin tool with Wasm sandbox stub

WHEN a Plugin tool is configured with `SandboxAttempt::Wasm { runtime: "wasmtime".into() }`
THEN `execute()` SHALL return `ToolOutput::error("WASM sandbox not yet implemented: runtime=wasmtime")`
AND the tool SHALL NOT be executed in the host process
