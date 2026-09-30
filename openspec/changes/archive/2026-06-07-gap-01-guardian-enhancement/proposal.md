## Why

Synthia's Guardian implementation lacks production-grade safety features. Current issues: SimpleGuardian has hardcoded rules only (rm/sudo detection), GuardianReviewer has no timeout or transcript compression, and CircuitBreaker only tracks compaction failures — not Guardian rejections. This creates safety gaps that could allow dangerous operations to pass through unchecked. Implementing the hybrid Guardian architecture will provide defense-in-depth with rule-based fast-path and LLM-based deep review.

## What Changes

**Guardian Architecture Enhancement**
- From: Single SimpleGuardian with hardcoded rules OR GuardianReviewer with full transcript
- To: Hybrid layer with SimpleGuardian (fast-path, rule-based) + GuardianReviewer (LLM-based, 30s timeout, compressed transcript) + CircuitBreaker (tracks Guardian rejections: 3 consecutive or 10 total → interrupt)
- Reason: Production-grade safety requires layered defense with graceful degradation
- Impact: Non-breaking — Guardian remains pluggable, new behavior is additive

**LLM Review Session Handling**
- From: Single-shot LLM call with full conversation, no timeout
- To: 30s timeout + transcript compression (keep recent N rounds + key context), fail-closed on timeout/error
- Reason: Prevent context window pressure and blocking on slow models
- Impact: Non-breaking — fallback behavior is explicit

**User Confirmation Mode**
- From: No differentiated confirmation strategy
- To: Action-type-based confirmation (Shell=blocking, Network=non-blocking, Credential=interrupt)
- Reason: Different operation types have different risk profiles and user availability expectations
- Impact: Non-breaking — configurable per operation type

**Hook Integration**
- From: No formal Guardian integration with hook system
- To: Hook calls Guardian.check() for all tool calls, Guardian returns Allow/Deny/NeedUserConfirm
- Reason: Guardian needs visibility into all operations for consistent safety enforcement
- Impact: Non-breaking — existing hooks unchanged, Guardian layered on top

## Capabilities

### New Capabilities

- `guardian-hybrid-layer`: Hybrid Guardian with SimpleGuardian (rule-based fast-path) and GuardianReviewer (LLM-based deep review) — implements fail-closed with degradation
- `guardian-circuit-breaker`: Guardian rejection tracking with 3 consecutive or 10 total denials triggering session interrupt
- `guardian-timeout-compression`: LLM review with 30s timeout and transcript compression to prevent context overflow
- `guardian-action-confirmation`: Action-type-based user confirmation (Shell=blocking, Network=async, Credential=interrupt)

### Modified Capabilities

- None — this is net-new capability addition

## Impact

- **Code**: `synthia-guardian/src/` — new hybrid architecture, enhanced CircuitBreaker
- **Integration**: Hook system (`synthia-agent/src/hooks.rs`) — Guardian.check() called from before_tool
- **Config**: Guardian settings (timeout, compression ratio, circuit breaker thresholds) via AgentConfig
- **Dependencies**: No new external dependencies — uses existing ModelProvider and HookRegistry