# tool-concurrency-trait Specification

## Purpose
TBD - created by archiving change synthia-gap-analysis-2026-06-07. Update Purpose after archive.
## Requirements
### Requirement: Tool trait SHALL expose is_concurrency_safe

The `Tool` trait in `synthia-tool/src/traits.rs` SHALL expose a method `is_concurrency_safe(&self) -> bool` with a default implementation returning `false`.

#### Scenario: Default behavior is false
- **WHEN** a custom `impl Tool for MyTool {}` exists without overriding `is_concurrency_safe`
- **THEN** `my_tool.is_concurrency_safe()` SHALL return `false`
- **THEN** the tool SHALL be treated as concurrency-unsafe by the scheduler

#### Scenario: Override returns true
- **WHEN** a builtin overrides `is_concurrency_safe` to return `true`
- **THEN** the scheduler SHALL permit parallel execution of multiple invocations of that tool

### Requirement: Read-only builtin tools SHALL declare concurrency safety

The following read-only builtins SHALL override `is_concurrency_safe` to return `true`: `read`, `glob`, `grep`, `web`, `path`.

#### Scenario: read is concurrency safe
- **WHEN** `ReadTool::is_concurrency_safe()` is called
- **THEN** it SHALL return `true`

#### Scenario: glob is concurrency safe
- **WHEN** `GlobTool::is_concurrency_safe()` is called
- **THEN** it SHALL return `true`

#### Scenario: grep is concurrency safe
- **WHEN** `GrepTool::is_concurrency_safe()` is called
- **THEN** it SHALL return `true`

#### Scenario: web is concurrency safe
- **WHEN** `WebTool::is_concurrency_safe()` is called
- **THEN** it SHALL return `true`

#### Scenario: path is concurrency safe
- **WHEN** `PathTool::is_concurrency_safe()` is called
- **THEN** it SHALL return `true` (read-only path operations)

### Requirement: Mutating builtin tools SHALL declare concurrency unsafe

The following mutating builtins SHALL NOT override `is_concurrency_safe` (default `false`): `bash`, `write`, `multi_edit`.

#### Scenario: bash is concurrency unsafe
- **WHEN** `BashTool::is_concurrency_safe()` is called
- **THEN** it SHALL return `false` (default implementation)

#### Scenario: write is concurrency unsafe
- **WHEN** `WriteTool::is_concurrency_safe()` is called
- **THEN** it SHALL return `false` (default implementation)

#### Scenario: multi_edit is concurrency unsafe
- **WHEN** `MultiEditTool::is_concurrency_safe()` is called
- **THEN** it SHALL return `false` (default implementation)

### Requirement: Step scheduler SHALL use is_concurrency_safe

The tool scheduler in `synthia-agent/src/agent/step.rs` SHALL use `is_concurrency_safe` to determine parallel execution. The hardcoded `false` value SHALL be removed.

#### Scenario: Parallel execution enabled for safe tools
- **WHEN** the LLM returns multiple `read` tool calls in one turn
- **THEN** the scheduler SHALL execute them in parallel
- **THEN** total execution time SHALL be approximately `max(read_times)` not `sum(read_times)`

#### Scenario: Serial execution for unsafe tools
- **WHEN** the LLM returns multiple `write` tool calls in one turn
- **THEN** the scheduler SHALL execute them serially
- **THEN** `parallel_task_dispatch_test` SHALL pass with serial mode verified

#### Scenario: Mixed parallel/serial
- **WHEN** the LLM returns a mix of `read` (safe) and `write` (unsafe) tool calls
- **THEN** `read` calls SHALL execute in parallel
- **THEN** `write` calls SHALL execute serially
- **THEN** no `read` and `write` SHALL execute concurrently (write requires exclusive session state)

### Requirement: Existing parallel_task_dispatch_test SHALL pass

The existing test `parallel_task_dispatch_test.rs` SHALL pass with the new scheduler logic, verifying that parallel execution actually happens (not just by coincidence of scheduling order).

#### Scenario: Test asserts parallel timing
- **WHEN** the test dispatches 4 `read` tool calls with 100ms sleep each
- **THEN** total wall-clock time SHALL be < 200ms (parallel) not 400ms (serial)
- **THEN** the test SHALL pass consistently across 10 runs

#### Scenario: Test asserts serial for unsafe
- **WHEN** the test dispatches 4 `bash` tool calls
- **THEN** total wall-clock time SHALL be ≥ 400ms (serial)
- **THEN** the test SHALL verify no two bash invocations ran concurrently

### Requirement: Backward compatibility SHALL be preserved

Existing third-party `impl Tool` implementations SHALL continue to compile and behave correctly without modification.

#### Scenario: Old impl Tool compiles
- **WHEN** a downstream crate has `impl Tool for MyTool {}` without `is_concurrency_safe`
- **THEN** compilation SHALL succeed (default method kicks in)
- **THEN** `MyTool::is_concurrency_safe()` SHALL return `false`

#### Scenario: Old impl behavior unchanged
- **WHEN** an old `MyTool` is registered in the tool registry
- **THEN** the scheduler SHALL treat it as concurrency-unsafe
- **THEN** behavior SHALL match pre-change semantics (serial execution)

