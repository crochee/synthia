# Spec: loop-detector-layered-integration

## ADDED Requirements

### Requirement: Layered loop detection architecture

The system SHALL use a two-layer loop detection architecture in main_loop:
- **Layer 1 (hard floor)**: `synthia-guardian::LoopDetectorSet` with 5 specialized detectors, running in `check_doom_loop`
- **Layer 2 (soft vote)**: `synthia-hook::LoopDetector` with similarity-based detection, running as a `Hook` via `UnifiedHookDispatcher`

Layer 1 results SHALL take precedence over Layer 2. If Layer 1 detects a loop, Layer 2 SHALL NOT override the detection.

#### Scenario: Both layers detect a loop

WHEN `LoopDetectorSet` detects a doom loop AND `synthia-hook::LoopDetector` also returns `LoopStatus::Detected`
THEN the main_loop SHALL treat it as a doom loop (Layer 1 result)
AND the `HookOutcome::Deny` from Layer 2 SHALL be logged but not change the outcome

#### Scenario: Layer 1 passes, Layer 2 detects

WHEN `LoopDetectorSet` returns `LoopStatus::Ok` AND `synthia-hook::LoopDetector` returns `HookOutcome::Deny { reason: "loop_detected" }`
THEN the main_loop SHALL deny the tool call (Layer 2 result)
AND the denial reason SHALL include the similarity threshold and window size

#### Scenario: Layer 1 detects, Layer 2 passes

WHEN `LoopDetectorSet` detects a doom loop AND `synthia-hook::LoopDetector` returns `HookOutcome::Allow`
THEN the main_loop SHALL treat it as a doom loop (Layer 1 result)
AND Layer 2's `Allow` SHALL be ignored

### Requirement: LoopDetector as Hook trait implementation

`synthia-hook::LoopDetector` SHALL implement the `Hook` trait (from PR-4.2) so it can be registered with `UnifiedHookDispatcher`.

#### Scenario: LoopDetector registered as Hook

WHEN `LoopDetector` is registered with `UnifiedHookDispatcher` via `hook_dispatcher.register(Arc::new(loop_detector))`
THEN the dispatcher SHALL call `loop_detector.on_event()` for `HookEvent::PreToolUse` and `HookEvent::PostToolUse`

#### Scenario: LoopDetector records PostToolUse events

WHEN a `PostToolUse` event is dispatched through `UnifiedHookDispatcher`
THEN `LoopDetector::on_event()` SHALL be called with the event
AND `LoopDetector` SHALL record the tool name and input hash in its history

### Requirement: LoopDetector configuration

The `LoopDetector` SHALL be configurable with `similarity_threshold` (default 0.9) and `window_size` (default 3).

#### Scenario: Custom similarity threshold

WHEN `LoopDetector::with_config(0.8, 3)` is used
THEN a tool call with ≥80% input similarity to the previous 3 calls SHALL trigger `LoopStatus::Warning`
AND a tool call with ≥80% input similarity matching all 3 previous calls SHALL trigger `LoopStatus::Detected`
