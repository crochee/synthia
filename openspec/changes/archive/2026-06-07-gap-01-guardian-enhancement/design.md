## Context

Synthia's Guardian is the central safety layer for the AI agent. Current implementation has significant gaps compared to production-grade agents (opencode, codex):

**Current State (Problems)**:
- `SimpleGuardian`: Hardcoded `rm`/`sudo` detection with risk scores 85-90. No session isolation, no escalation path.
- `GuardianReviewer`: Single LLM call with full transcript, no timeout, no compression. LLM failure = no safety.
- `CircuitBreaker`: Only tracks compaction failures (`consecutive_compact_failures`). No Guardian rejection tracking.

**Stakeholders**: Agent safety, session integrity, user trust.

**Constraints**:
- Must integrate with existing Hook system (`before_llm`, `after_llm`, `before_tool`)
- Must not block normal operations (false positives are UX friction)
- Must fail-safe (dangerous operations denied > allowed)

## Goals / Non-Goals

**Goals:**
- Implement hybrid Guardian layer (rule-based fast-path + LLM deep review)
- Add LLM timeout (30s) with compressed transcript to prevent blocking
- Track Guardian rejections in CircuitBreaker (3 consecutive / 10 total → interrupt)
- Differentiate user confirmation by action type (Shell/Network/Credential)
- Maintain graceful degradation (LLM fail → SimpleGuardian fallback)

**Non-Goals:**
- Replace existing Hook system — Guardian layers on top
- Modify session store schema
- Add new external dependencies
- Change existing agent configuration format

## Decisions

### D1: Architecture — Hybrid Guardian Layer

- **選擇**：Option C — Hybrid: Local SimpleGuardian for fast-path, centralized GuardianReviewer for complex cases
- **理由**：Matches Synthia's hook system architecture, enables graceful degradation, aligns with opencode's production pattern
- **已考慮 alternative**:
  - Option A (Centralized only): Single point of failure, latency for every check
  - Option B (Per-agent only): Inconsistent policy enforcement, hard to audit

### D2: LLM Session — Timeout + Compressed Transcript

- **選擇**：30s timeout + compressed transcript (recent N rounds + key context) + fail-closed
- **理由**：Prevents context window pressure, prevents blocking on slow models, fail-closed ensures safety
- **已考慮 alternative**:
  - Single-shot (current): No timeout = potential blocking forever
  - Multi-turn: Over-engineered, LLM review is single assessment not dialogue

### D3: Circuit Breaker — opencode Pattern

- **選擇**：3 consecutive denials OR 10 total denials → session interrupt
- **理由**：Matches opencode production proven pattern, clear threshold for user intervention
- **已考慮 alternative**:
  - Compaction-only tracking (current): Missing Guardian rejection tracking entirely
  - Sliding window: More complex, no clear benefit over fixed thresholds

### D4: User Confirmation — Action-Type Differentiation

- **選擇**：Configurable per action type: Shell/Exec=blocking, Network=non-blocking, Credential=interrupt
- **理由**：Different operations have different risk profiles and urgency — Shell destructive vs Network informational
- **已考慮 alternative**:
  - All blocking: Over-synchronous, user annoyed for low-risk ops
  - All non-blocking: Dangerous ops may proceed without confirmation

### D5: Hook Integration — Guardian as Coordinator

- **選擇**：Guardian as upper-layer coordinator, hook calls `Guardian.check()` for all tool operations
- **理由**：Guardian needs visibility into all operations, hook system provides entry point, separation of concerns maintained
- **已考慮 alternative**:
  - Independent safety layer: Duplicates hook visibility
  - Hook calls Guardian directly: Tight coupling, harder to test

### D6: Fail-Closed Strategy — Degrade to SimpleGuardian

- **選擇**：LLM failure → SimpleGuardian fallback; Service unavailable → deny
- **理由**：Maintains basic safety on LLM failure without blocking all operations; service unavailable = conservative deny
- **已考慮 alternative**:
  - Pure fail-open: Dangerous
  - Pure fail-closed: Blocks normal operations when LLM unavailable

## Risks / Trade-offs

[Risk] LLM timeout causes false positives for legitimate complex operations → Mitigation: SimpleGuardian fallback preserves basic safety; 30s is generous for review
[Risk] Transcript compression loses critical context → Mitigation: Compression preserves system prompt + recent N rounds + ruleset summary
[Risk] Circuit breaker too aggressive → Mitigation: Thresholds (3/10) match production-proven opencode pattern
[Risk] User confirmation UX friction → Mitigation: Non-blocking for Network operations; only Shell/Credential are synchronous

[Trade-off] Complexity vs Safety: Hybrid architecture more complex than single Guardian → 接受理由: Defense-in-depth justified for production safety
[Trade-off] Latency vs Safety: LLM review adds latency → 接受理由: 30s timeout bounds latency; safety > speed for high-risk operations

## Migration Plan

N/A — This change does not involve deployment or endpoint changes. It adds new capability to synthia-guardian crate.

**Implementation order**:
1. Add `GuardianCircuitBreaker` tracking (3 consecutive / 10 total)
2. Implement `GuardianReviewer` with timeout + compression
3. Add `GuardianDecision` enum (Allow/Deny/NeedUserConfirm)
4. Integrate Guardian into Hook system (`before_tool`)
5. Add action-type confirmation routing

**Verification**:
- Unit tests for each component
- Integration test: Verify Guardian.check() called from hook
- Circuit breaker test: 3 denials → interrupt

## Open Questions

1. **Compression ratio**: How many recent rounds to preserve? (Default: 10)
2. **SimpleGuardian ruleset**: What rules beyond rm/sudo should be included? (Need security team input)
3. **Confirmation timeout**: How long to wait for user confirmation before deny? (Default: 5min)
4. **Logging**: Should Guardian decisions be logged to session store for audit? (Yes, but schema TBD)