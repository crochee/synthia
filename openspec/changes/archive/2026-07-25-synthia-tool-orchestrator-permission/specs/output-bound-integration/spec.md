# Spec: output-bound-integration

## ADDED Requirements

### Requirement: OutputBound::bind() in execute_and_emit Phase 4

`execute_and_emit` Phase 4 SHALL call `OutputBound::bind()` on tool output instead of `truncate_output`.

#### Scenario: Output within bounds

WHEN a tool produces output of 30 KiB (below 50 KiB cap)
THEN `OutputBound::bind()` SHALL return `BoundOutput` with `truncated: false`
AND the full output SHALL be included in the `ToolCallCompleted` event

#### Scenario: Output exceeds byte cap

WHEN a tool produces output of 80 KiB (above 50 KiB cap)
THEN `OutputBound::bind()` SHALL return `BoundOutput` with `truncated: true`
AND the output SHALL be truncated to 50 KiB
AND `original_len` SHALL be 81920

#### Scenario: Output exceeds line cap

WHEN a tool produces output with 3000 lines (above 2000 line cap)
THEN `OutputBound::bind()` SHALL return `BoundOutput` with `truncated: true`
AND the output SHALL be truncated to 2000 lines

### Requirement: OutputBound instance from LoopServices

The `OutputBound` instance SHALL come from `LoopServices.output_bound`.

#### Scenario: LoopServices provides OutputBound

WHEN `LoopServices` is configured with `output_bound: Some(Arc::new(DefaultOutputBound::new(config)))`
THEN `execute_and_emit` SHALL use this instance for bounding

#### Scenario: No OutputBound configured

WHEN `LoopServices.output_bound` is `None`
THEN `execute_and_emit` SHALL use `DefaultOutputBound::default()` as fallback
