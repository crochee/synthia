## 1. Guardian Core Types

- [x] 1.1 Add `GuardianDecision` enum with `Allow`, `Deny { reason }`, `NeedUserConfirm { request, timeout, blocking, action_type }`
- [x] 1.2 Add `Guardian` trait with `async fn check(&self, request: &ApprovalRequest) -> GuardianDecision`
- [x] 1.3 Add `ActionType` enum with `Shell`, `Network`, `Credential` variants

## 2. SimpleGuardian Enhancement

- [x] 2.1 Extend SimpleGuardian ruleset (beyond rm/sudo: chmod 777, curl with headers, env var access)
- [x] 2.2 Implement risk scoring (0-100) with threshold-based routing
- [x] 2.3 Add `check` method implementing `Guardian` trait

## 3. GuardianReviewer with Timeout + Compression

- [x] 3.1 Implement `GuardianReviewer` struct with model provider, timeout (30s), transcript_limit (10 rounds)
- [x] 3.2 Implement transcript compression (preserve system prompt + recent N rounds + summary)
- [x] 3.3 Implement `build_review_prompt` with task context, approval request, compressed history, risk criteria
- [x] 3.4 Implement `check` method with 30s timeout, fail-closed on timeout/error

## 4. GuardianCircuitBreaker Enhancement

- [x] 4.1 Add `GuardianCircuitBreaker` tracking `consecutive_denials`, `total_denials`, `session_interrupt`
- [x] 4.2 Implement `record_denial()` — increment counters, check thresholds, set interrupt if threshold reached
- [x] 4.3 Implement `record_approval()` — reset consecutive_denials
- [x] 4.4 Implement `should_interrupt()` — return true if 3 consecutive OR 10 total
- [x] 4.5 Implement `reset()` for session restart

## 5. GuardianCoordinator (Hybrid Layer)

- [x] 5.1 Implement `GuardianCoordinator` that combines SimpleGuardian + GuardianReviewer + CircuitBreaker
- [x] 5.2 Implement fast-path: SimpleGuardian check first, only escalate if risk >= 50
- [x] 5.3 Implement degradation: LLM fail → SimpleGuardian fallback, service fail → deny
- [x] 5.4 Wire circuit breaker: denial → record_denial, approval → record_approval, check should_interrupt before each request

## 6. Hook System Integration

- [x] 6.1 Add `Guardian::check()` call from `before_tool` hook
- [x] 6.2 Route ToolAction based on GuardianDecision: Allow→Proceed, Deny→Skip, NeedUserConfirm→PendingConfirm
- [x] 6.3 Integrate circuit breaker state check at iteration start

## 7. User Confirmation Flow

- [x] 7.1 Implement action-type detection (Shell/Network/Credential from ApprovalRequest)
- [x] 7.2 Implement blocking confirmation for Shell (agent pauses)
- [x] 7.3 Implement non-blocking confirmation for Network (agent continues)
- [x] 7.4 Implement interrupt confirmation for Credential (session checkpoint + pause)
- [x] 7.5 Implement confirmation timeout (5min default) with deny on expiry

## 8. Configuration

- [x] 8.1 Add Guardian config to AgentConfig: timeout_ms, compression_rounds, circuit_breaker_thresholds
- [x] 8.2 Add confirmation timeout config per action type
- [x] 8.3 Wire config into GuardianCoordinator initialization

## 9. Testing

- [x] 9.1 Unit tests for GuardianDecision enum and trait
- [x] 9.2 Unit tests for SimpleGuardian risk scoring
- [x] 9.3 Unit tests for GuardianReviewer timeout behavior
- [x] 9.4 Unit tests for GuardianCircuitBreaker threshold tracking
- [x] 9.5 Integration test: Guardian.check() called from hook, decision routing works
- [x] 9.6 Circuit breaker test: 3 denials → should_interrupt returns true