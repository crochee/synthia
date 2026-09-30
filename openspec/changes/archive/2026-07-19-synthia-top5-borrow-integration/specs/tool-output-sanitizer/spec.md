# Capability: tool-output-sanitizer

> **Status**: Proposed (change #1: 架构基础设施)
> **Source**: opencode `packages/core/src/tool/outputBound.ts` + `truncate.ts:158`

## Purpose

在 `synthia-tool-materialization` 下挂 `OutputBound` 60 行实现，提供 Contentlen histogram + 50KiB/2K 行 cap，配合 `synthia-event-v2::cleanup.rs` 7d retention + `ToolContext::take_output` (50 行)。注意：tree-sitter shell AST permission 在 change #3，不在此处。

## ADDED Requirements

### Requirement: OutputBound trait

The `synthia-tool-materialization` crate MUST expose an `OutputBound` trait providing `bind` / `content_len` / `cleanup` methods.

#### Scenario: default bound

- **WHEN** `OutputBound::bind(&self, output: Vec<u8>)` is called
- **THEN** the trait MUST truncate the output to 50KiB (default) or 2000 lines, whichever is smaller
- **AND** MUST emit a `truncated` marker when truncation occurs

#### Scenario: configurable cap

- **WHEN** `OutputBoundConfig::with_max_bytes(128 * 1024)` is applied
- **THEN** the cap MUST be 128KiB instead of the default 50KiB

### Requirement: 7-day retention CleanupTask

The `synthia-event-v2::cleanup::CleanupTask` MUST enforce 7-day retention on tool outputs alongside events.

#### Scenario: cleanup runs hourly

- **WHEN** the runtime is started with `event-v2` and `tool-output-sanitizer` features
- **THEN** `CleanupTask` MUST spawn every 3600s
- **AND** MUST delete tool outputs older than `now - 7 days` from the dual table
- **AND** MUST keep the corresponding event headers

#### Scenario: cleanup disables outputs

- **WHEN** `SYNTHIA_TOOL_OUTPUT_RETENTION_DAYS=0` is set
- **THEN** the `CleanupTask` MUST NOT delete any tool outputs
- **AND** the cleanup loop MUST remain active (heartbeat only)

### Requirement: ToolContext::take_output

The `ToolContext` MUST expose a `take_output() -> Option<Vec<u8>>` that drains the captured buffer.

#### Scenario: take_output consumes buffer

- **WHEN** `tool_ctx.take_output()` is called
- **THEN** the internal buffer MUST be replaced with `None`
- **AND** subsequent calls MUST return `None` until a new output is bound
- **AND** MUST NOT block

#### Scenario: take_output with retention

- **WHEN** `take_output()` returns Some(buf) and retention is configured
- **THEN** the buffer MUST be persisted to the cleanup-managed table
- **AND** MUST be subject to 7-day eviction

### Requirement: CachePolicyApplier Arc::ptr_eq short-circuit preserved

The existing `CachePolicyApplier` `Arc::ptr_eq` zero-copy short-circuit MUST remain on the truncation path.

#### Scenario: identical Arc skipped

- **WHEN** two consecutive truncations operate on the same `Arc<Vec<u8>>`
- **THEN** the second truncation MUST detect `Arc::ptr_eq` and skip re-cloning
- **AND** MUST emit a metrics counter `tool_output_arc_ptr_eq_hit_total`
