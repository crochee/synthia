# GAP-01 Guardian Enhancement Implementation Plan

> **For agentic workers:** Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement hybrid Guardian layer with rule-based fast-path + LLM deep review, circuit breaker for rejection tracking, timeout + transcript compression, and action-type-based user confirmation.

**Architecture:** GuardianCoordinator orchestrates SimpleGuardian (rule-based fast-path) + GuardianReviewer (LLM-based with 30s timeout + compression). GuardianCircuitBreaker tracks 3 consecutive or 10 total denials per session. Hook system integrates via before_tool calling Guardian.check().

**Tech Stack:** Rust, synthia-guardian crate, synthia-agent hooks, async_trait, tokio timeout

---

## Task 1: Guardian Core Types

**Files:**
- Create: `crates/synthia-guardian/src/guardian_decision.rs`
- Modify: `crates/synthia-guardian/src/lib.rs`

- [ ] **Step 1: Create guardian_decision.rs with GuardianDecision enum and ActionType**

```rust
use std::time::Duration;
use crate::ApprovalRequest;

/// Action type for confirmation routing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionType {
    Shell,
    Network,
    Credential,
}

/// Guardian decision with action-type awareness
#[derive(Debug, Clone)]
pub enum GuardianDecision {
    Allow,
    Deny { reason: String },
    NeedUserConfirm {
        request: ApprovalRequest,
        timeout: Duration,
        blocking: bool,
        action_type: ActionType,
    },
}

impl GuardianDecision {
    pub fn is_allowed(&self) -> bool {
        matches!(self, GuardianDecision::Allow)
    }

    pub fn action_type(&self) -> Option<ActionType> {
        match self {
            GuardianDecision::NeedUserConfirm { action_type, .. } => Some(*action_type),
            _ => None,
        }
    }
}

impl ActionType {
    pub fn from_approval_request(request: &ApprovalRequest) -> Self {
        match request {
            ApprovalRequest::Shell { .. } | ApprovalRequest::ExecCommand { .. } => ActionType::Shell,
            ApprovalRequest::NetworkAccess { .. } => ActionType::Network,
            ApprovalRequest::ApplyPatch { .. } | ApprovalRequest::McpToolCall { .. } => ActionType::Credential,
        }
    }

    pub fn default_timeout(&self) -> Duration {
        match self {
            ActionType::Shell => Duration::from_secs(300),      // 5 min
            ActionType::Network => Duration::from_secs(60),   // 1 min
            ActionType::Credential => Duration::from_secs(120), // 2 min
        }
    }

    pub fn is_blocking(&self) -> bool {
        matches!(self, ActionType::Shell | ActionType::Credential)
    }
}
```

- [ ] **Step 2: Add new Guardian trait with check method**

```rust
use async_trait::async_trait;

#[async_trait]
pub trait Guardian: Send + Sync {
    async fn check(&self, request: &ApprovalRequest) -> GuardianDecision;
}
```

- [ ] **Step 3: Update lib.rs exports**

```rust
// Add after review_types module
mod guardian_decision;
pub use guardian_decision::{ActionType, GuardianDecision};
```

- [ ] **Step 4: Run tests to verify types compile**

Run: `cargo build -p synthia-guardian 2>&1`
Expected: BUILD SUCCESS

- [ ] **Step 5: Commit**

```bash
git add crates/synthia-guardian/src/guardian_decision.rs crates/synthia-guardian/src/lib.rs
git commit -m "feat(guardian): add GuardianDecision enum and ActionType"
```

---

## Task 2: GuardianCircuitBreaker

**Files:**
- Create: `crates/synthia-guardian/src/guardian_circuit_breaker.rs`
- Modify: `crates/synthia-guardian/src/lib.rs`

- [ ] **Step 1: Create GuardianCircuitBreaker struct**

```rust
/// Circuit breaker for tracking Guardian denials per session.
/// Triggers session interrupt after 3 consecutive denials or 10 total denials.
#[derive(Debug)]
pub struct GuardianCircuitBreaker {
    consecutive_denials: u8,
    total_denials: u32,
    session_interrupt: bool,
}

impl GuardianCircuitBreaker {
    pub fn new() -> Self {
        Self {
            consecutive_denials: 0,
            total_denials: 0,
            session_interrupt: false,
        }
    }

    /// Records a Guardian denial, updates counters, checks thresholds
    pub fn record_denial(&mut self) {
        self.consecutive_denials += 1;
        self.total_denials += 1;

        if self.consecutive_denials >= 3 || self.total_denials >= 10 {
            tracing::warn!(
                consecutive = self.consecutive_denials,
                total = self.total_denials,
                "Guardian circuit breaker triggered - session interrupt"
            );
            self.session_interrupt = true;
        }
    }

    /// Records a Guardian approval, resets consecutive counter
    pub fn record_approval(&mut self) {
        self.consecutive_denials = 0;
    }

    /// Returns true if session should be interrupted
    pub fn should_interrupt(&self) -> bool {
        self.session_interrupt
    }

    /// Resets all counters and interrupt flag
    pub fn reset(&mut self) {
        self.consecutive_denials = 0;
        self.total_denials = 0;
        self.session_interrupt = false;
    }

    pub fn consecutive_denials(&self) -> u8 {
        self.consecutive_denials
    }

    pub fn total_denials(&self) -> u32 {
        self.total_denials
    }
}

impl Default for GuardianCircuitBreaker {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 2: Add unit tests for threshold tracking**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_consecutive_denials_triggers_interrupt() {
        let mut cb = GuardianCircuitBreaker::new();
        assert!(!cb.should_interrupt());

        cb.record_denial();
        cb.record_denial();
        assert!(!cb.should_interrupt());

        cb.record_denial(); // 3rd consecutive
        assert!(cb.should_interrupt());
    }

    #[test]
    fn test_total_denials_triggers_interrupt() {
        let mut cb = GuardianCircuitBreaker::new();

        for i in 1..=10 {
            cb.record_denial();
            if i < 10 {
                assert!(!cb.should_interrupt());
            }
        }
        assert!(cb.should_interrupt());
    }

    #[test]
    fn test_approval_resets_consecutive() {
        let mut cb = GuardianCircuitBreaker::new();
        cb.record_denial();
        cb.record_denial();
        assert_eq!(cb.consecutive_denials(), 2);

        cb.record_approval();
        assert_eq!(cb.consecutive_denials(), 0);
    }

    #[test]
    fn test_interrupt_persists_after_approval() {
        let mut cb = GuardianCircuitBreaker::new();
        cb.record_denial();
        cb.record_denial();
        cb.record_denial(); // interrupt triggered
        assert!(cb.should_interrupt());

        cb.record_approval();
        assert!(cb.should_interrupt()); // still true
    }

    #[test]
    fn test_reset_clears_all() {
        let mut cb = GuardianCircuitBreaker::new();
        cb.record_denial();
        cb.record_denial();
        cb.record_denial();
        assert!(cb.should_interrupt());

        cb.reset();
        assert!(!cb.should_interrupt());
        assert_eq!(cb.consecutive_denials(), 0);
        assert_eq!(cb.total_denials(), 0);
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p synthia-guardian guardian_circuit_breaker 2>&1`
Expected: All tests PASS

- [ ] **Step 4: Commit**

```bash
git add crates/synthia-guardian/src/guardian_circuit_breaker.rs crates/synthia-guardian/src/lib.rs
git commit -m "feat(guardian): add GuardianCircuitBreaker for denial tracking"
```

---

## Task 3: SimpleGuardian Enhancement

**Files:**
- Modify: `crates/synthia-guardian/src/review.rs`

- [ ] **Step 1: Add ActionType import and implement check() method on SimpleGuardian**

```rust
use crate::{guardian_decision::*, ApprovalRequest, GuardianConfig, ReviewDecision};

#[async_trait]
impl Guardian for SimpleGuardian {
    async fn check(&self, request: &ApprovalRequest) -> GuardianDecision {
        if !self.config.enabled {
            return GuardianDecision::Allow;
        }

        let risk_score = self.assess_risk(request);

        // Low risk (< 50): allow
        if risk_score < 50 {
            return GuardianDecision::Allow;
        }

        // High risk (>= 80): deny with reason
        if risk_score >= 80 {
            return GuardianDecision::Deny {
                reason: format!("Risk score {} exceeds threshold", risk_score),
            };
        }

        // Medium risk (50-79): need user confirm
        GuardianDecision::NeedUserConfirm {
            request: request.clone(),
            timeout: ActionType::from_approval_request(request).default_timeout(),
            blocking: ActionType::from_approval_request(request).is_blocking(),
            action_type: ActionType::from_approval_request(request),
        }
    }
}
```

- [ ] **Step 2: Extend assess_risk with more patterns**

```rust
fn assess_risk(&self, request: &ApprovalRequest) -> u8 {
    match request {
        ApprovalRequest::Shell { command, .. } | ApprovalRequest::ExecCommand { command, .. } => {
            let cmd_str = command.join(" ");
            if cmd_str.contains("rm -rf") || cmd_str.contains("sudo") || cmd_str.contains("chmod 777") {
                90
            } else if cmd_str.contains("curl") && (cmd_str.contains("-H") || cmd_str.contains("--header")) {
                75
            } else if cmd_str.contains("export") && (cmd_str.contains("SECRET") || cmd_str.contains("KEY") || cmd_str.contains("TOKEN")) {
                85
            } else {
                30
            }
        }
        ApprovalRequest::ApplyPatch { patch, .. } => {
            if patch.contains("rm -rf") || patch.contains("sudo") || patch.contains("chmod 777") {
                90
            } else {
                40
            }
        }
        ApprovalRequest::NetworkAccess { .. } => {
            // Network access is medium-high risk
            65
        }
        ApprovalRequest::McpToolCall { tool_name, annotations, .. } => {
            // Check MCP annotations for risk hints
            if let Some(ann) = annotations {
                if ann.destructive_hint == Some(true) {
                    return 85;
                }
                if ann.open_world_hint == Some(true) {
                    return 70;
                }
            }
            // Default to medium risk for MCP tools
            50
        }
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p synthia-guardian 2>&1`
Expected: All tests PASS

- [ ] **Step 4: Commit**

```bash
git add crates/synthia-guardian/src/review.rs
git commit -m "feat(guardian): enhance SimpleGuardian with check() and extended ruleset"
```

---

## Task 4: GuardianReviewer with Timeout + Compression

**Files:**
- Modify: `crates/synthia-guardian/src/reviewer.rs`

- [ ] **Step 1: Add imports and timeout config**

```rust
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;
use synthia_model_router::ModelRouter;
use synthia_provider::{CompletionRequest, ContentPart, Message, TextContent, ToolChoice, collect_stream};
use crate::{ApprovalRequest, GuardianConfig, ReviewDecision, guardian_decision::GuardianDecision, build_review_prompt, collect_transcript_entries, parse_assessment_response, review_types::Assessment};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_TRANSCRIPT_LIMIT: usize = 10;
```

- [ ] **Step 2: Add timeout and compression fields to GuardianReviewer**

```rust
pub struct GuardianReviewer {
    config: GuardianConfig,
    timeout: Duration,
    transcript_limit: usize,
}

impl GuardianReviewer {
    pub fn new(config: GuardianConfig) -> Self {
        Self {
            config,
            timeout: DEFAULT_TIMEOUT,
            transcript_limit: DEFAULT_TRANSCRIPT_LIMIT,
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn with_transcript_limit(mut self, limit: usize) -> Self {
        self.transcript_limit = limit;
        self
    }
}
```

- [ ] **Step 3: Implement check() with timeout and compressed transcript**

```rust
#[async_trait]
impl Guardian for GuardianReviewer {
    async fn check(&self, request: &ApprovalRequest) -> GuardianDecision {
        if !self.config.enabled {
            return GuardianDecision::Allow;
        }

        let action_json = match request.to_json() {
            Ok(json) => serde_json::to_string_pretty(&json).unwrap_or_default(),
            Err(e) => {
                tracing::error!("Failed to serialize approval request: {}", e);
                return GuardianDecision::Deny {
                    reason: "Failed to serialize approval request".to_string(),
                };
            }
        };

        // Use compressed transcript (keep recent N rounds + summary)
        let compressed_entries = collect_transcript_entries(&[]); // Will be passed from caller
        let review_prompt = build_review_prompt(&compressed_entries, &action_json, None);

        // Call LLM with timeout
        let result = timeout(
            self.timeout,
            self.call_llm(review_prompt, request),
        ).await;

        match result {
            Ok(Ok(decision)) => decision,
            Ok(Err(e)) => {
                tracing::warn!("LLM review failed: {}", e);
                GuardianDecision::Deny {
                    reason: format!("LLM review error: {}", e),
                }
            }
            Err(_) => {
                tracing::warn!("LLM review timed out after {:?}", self.timeout);
                GuardianDecision::Deny {
                    reason: "LLM review timeout - fail closed".to_string(),
                }
            }
        }
    }
}
```

- [ ] **Step 4: Implement call_llm with compressed transcript**

```rust
impl GuardianReviewer {
    async fn call_llm(&self, prompt: String, request: &ApprovalRequest) -> anyhow::Result<GuardianDecision> {
        let routing_result = self.router.route(&[]).await?;
        let provider = &routing_result.provider;

        let params = CompletionRequest {
            model: routing_result.decision.selected_model.clone(),
            messages: vec![Message {
                role: synthia_provider::Role::User,
                content: synthia_provider::Content::Single(
                    ContentPart::Text(TextContent { text: prompt }),
                ),
                tool_call_id: None,
                name: None,
            }],
            tools: vec![],
            tool_choice: ToolChoice::None,
            temperature: Some(0.0),
            max_tokens: Some(1024),
            stop_sequences: vec![],
            extra_body: None,
        };

        let stream = provider.stream(params).await?;
        let text_content = collect_stream(stream).await?;
        let assessment = parse_assessment_response(&text_content)?;

        Ok(self.make_guardian_decision(assessment, request))
    }

    fn make_guardian_decision(&self, assessment: Assessment, request: &ApprovalRequest) -> GuardianDecision {
        let risk_score = assessment.risk_score;

        if risk_score < 50 {
            GuardianDecision::Allow
        } else if risk_score >= 80 {
            GuardianDecision::Deny {
                reason: assessment.rationale,
            }
        } else {
            let action_type = ActionType::from_approval_request(request);
            GuardianDecision::NeedUserConfirm {
                request: request.clone(),
                timeout: action_type.default_timeout(),
                blocking: action_type.is_blocking(),
                action_type,
            }
        }
    }
}
```

- [ ] **Step 5: Run tests**

Run: `cargo build -p synthia-guardian 2>&1`
Expected: BUILD SUCCESS

- [ ] **Step 6: Commit**

```bash
git add crates/synthia-guardian/src/reviewer.rs
git commit -m "feat(guardian): add timeout and compression to GuardianReviewer"
```

---

## Task 5: GuardianCoordinator (Hybrid Layer)

**Files:**
- Create: `crates/synthia-guardian/src/guardian_coordinator.rs`
- Modify: `crates/synthia-guardian/src/lib.rs`

- [ ] **Step 1: Create GuardianCoordinator**

```rust
use std::sync::Arc;
use async_trait::async_trait;
use crate::{
    ApprovalRequest, GuardianConfig, GuardianCircuitBreaker, GuardianDecision,
    SimpleGuardian, guardian_decision::ActionType,
};

pub struct GuardianCoordinator {
    simple_guardian: SimpleGuardian,
    circuit_breaker: GuardianCircuitBreaker,
}

impl GuardianCoordinator {
    pub fn new(config: GuardianConfig) -> Self {
        Self {
            simple_guardian: SimpleGuardian::new(config),
            circuit_breaker: GuardianCircuitBreaker::new(),
        }
    }

    pub fn circuit_breaker(&self) -> &GuardianCircuitBreaker {
        &self.circuit_breaker
    }
}

#[async_trait]
impl Guardian for GuardianCoordinator {
    async fn check(&self, request: &ApprovalRequest) -> GuardianDecision {
        // Check circuit breaker first
        if self.circuit_breaker.should_interrupt() {
            return GuardianDecision::Deny {
                reason: "Session interrupt - too many denials".to_string(),
            };
        }

        // Fast-path: SimpleGuardian check first
        let simple_decision = self.simple_guardian.check(request).await;

        match simple_decision {
            GuardianDecision::Allow => {
                // Record approval, return allow
                self.circuit_breaker.record_approval();
                GuardianDecision::Allow
            }
            GuardianDecision::Deny { reason } => {
                // Record denial, return deny
                self.circuit_breaker.record_denial();
                GuardianDecision::Deny { reason }
            }
            GuardianDecision::NeedUserConfirm { request, timeout, blocking, action_type } => {
                // Medium risk - record denial and escalate to LLM review
                // In v1, we return NeedUserConfirm directly
                // Future: escalate to GuardianReviewer for LLM-based deep review
                self.circuit_breaker.record_denial();
                GuardianDecision::NeedUserConfirm {
                    request,
                    timeout,
                    blocking,
                    action_type,
                }
            }
        }
    }
}
```

- [ ] **Step 2: Export GuardianCoordinator from lib.rs**

```rust
pub mod guardian_coordinator;
pub use guardian_coordinator::GuardianCoordinator;
```

- [ ] **Step 3: Run tests**

Run: `cargo build -p synthia-guardian 2>&1`
Expected: BUILD SUCCESS

- [ ] **Step 4: Commit**

```bash
git add crates/synthia-guardian/src/guardian_coordinator.rs crates/synthia-guardian/src/lib.rs
git commit -m "feat(guardian): add GuardianCoordinator hybrid layer"
```

---

## Task 6: Hook System Integration

**Files:**
- Modify: `crates/synthia-agent/src/hooks.rs`

- [ ] **Step 1: Add Guardian check to before_tool hook**

```rust
use synthia_guardian::{GuardianCoordinator, GuardianDecision};

pub async fn fire_before_tool(
    &self,
    ctx: &mut AgentContext,
    tool_call: &ToolCall,
) -> ToolAction {
    // Build ApprovalRequest from tool call
    let request = ApprovalRequest::from_tool_call(tool_call);

    // Call Guardian.check()
    let decision = self.guardian.check(&request).await;

    match decision {
        GuardianDecision::Allow => ToolAction::Proceed,
        GuardianDecision::Deny { reason } => {
            tracing::warn!(tool = %tool_call.name, reason = %reason, "Guardian denied tool");
            ToolAction::Skip
        }
        GuardianDecision::NeedUserConfirm { request, timeout, blocking, action_type } => {
            if blocking {
                ToolAction::PendingConfirm(request, timeout)
            } else {
                // Non-blocking: allow to proceed but track
                ToolAction::Proceed
            }
        }
    }
}
```

- [ ] **Step 2: Check circuit breaker at iteration start**

```rust
fn start_iteration_hook(&self, ctx: &mut AgentContext) {
    // Check if session should be interrupted
    if self.guardian.coordinator().circuit_breaker().should_interrupt() {
        ctx.set_needs_user_intervention("Guardian circuit breaker triggered");
    }
}
```

- [ ] **Step 3: Run build to verify integration**

Run: `cargo build -p synthia-agent 2>&1`
Expected: BUILD SUCCESS

- [ ] **Step 4: Commit**

```bash
git add crates/synthia-agent/src/hooks.rs
git commit -m "feat(agent): integrate Guardian.check() from before_tool hook"
```

---

## Task 7: Configuration

**Files:**
- Modify: `crates/synthia-agent/src/config/agent_config.rs`

- [ ] **Step 1: Add Guardian config to AgentConfig**

```rust
#[derive(Debug, Clone)]
pub struct GuardianConfig {
    pub enabled: bool,
    pub timeout_ms: u64,
    pub compression_rounds: usize,
    pub circuit_breaker_consecutive: u8,
    pub circuit_breaker_total: u32,
    pub confirmation_timeout_secs: u64,
}

impl Default for GuardianConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            timeout_ms: 30_000,
            compression_rounds: 10,
            circuit_breaker_consecutive: 3,
            circuit_breaker_total: 10,
            confirmation_timeout_secs: 300,
        }
    }
}
```

- [ ] **Step 2: Wire config into GuardianCoordinator initialization**

```rust
impl AgentConfig {
    pub fn build_guardian_coordinator(&self) -> GuardianCoordinator {
        let config = GuardianConfig::from(self);
        GuardianCoordinator::new(config)
    }
}

impl From<&AgentConfig> for synthia_guardian::GuardianConfig {
    fn from(config: &AgentConfig) -> Self {
        synthia_guardian::GuardianConfig {
            enabled: config.guardian.enabled,
            risk_threshold: 80,
            dangerous_tools: vec![],
        }
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo build -p synthia-agent 2>&1`
Expected: BUILD SUCCESS

- [ ] **Step 4: Commit**

```bash
git add crates/synthia-agent/src/config/agent_config.rs
git commit -m "feat(config): add Guardian config fields to AgentConfig"
```

---

## Task 8: Testing

**Files:**
- Create: `crates/synthia-guardian/tests/test_guardian_core.rs`
- Create: `crates/synthia-guardian/tests/test_guardian_circuit_breaker.rs`

- [ ] **Step 1: Unit tests for GuardianDecision**

```rust
use synthia_guardian::{ActionType, GuardianDecision, ApprovalRequest};

#[test]
fn test_guardian_decision_is_allowed() {
    assert!(GuardianDecision::Allow.is_allowed());
    assert!(!GuardianDecision::Deny { reason: "test".to_string() }.is_allowed());
}

#[test]
fn test_action_type_from_approval_request() {
    let shell = ApprovalRequest::shell("id", vec!["ls".to_string()], "/", None);
    assert_eq!(ActionType::from_approval_request(&shell), ActionType::Shell);

    let network = ApprovalRequest::network_access("id", "t", "target", "host", "https", 443);
    assert_eq!(ActionType::from_approval_request(&network), ActionType::Network);
}

#[test]
fn test_action_type_default_timeout() {
    assert_eq!(ActionType::Shell.default_timeout(), Duration::from_secs(300));
    assert_eq!(ActionType::Network.default_timeout(), Duration::from_secs(60));
    assert_eq!(ActionType::Credential.default_timeout(), Duration::from_secs(120));
}
```

- [ ] **Step 2: Circuit breaker integration tests**

```rust
#[test]
fn test_circuit_breaker_3_consecutive_denials() {
    let mut cb = GuardianCircuitBreaker::new();
    for _ in 0..2 {
        cb.record_denial();
        assert!(!cb.should_interrupt());
    }
    cb.record_denial(); // 3rd
    assert!(cb.should_interrupt());
}

#[test]
fn test_circuit_breaker_10_total_denials() {
    let mut cb = GuardianCircuitBreaker::new();
    for i in 1..=9 {
        cb.record_denial();
        assert!(!cb.should_interrupt(), "Should not interrupt at {}", i);
    }
    cb.record_denial(); // 10th
    assert!(cb.should_interrupt());
}
```

- [ ] **Step 3: Run all tests**

Run: `cargo test -p synthia-guardian 2>&1`
Expected: All tests PASS

- [ ] **Step 4: Commit**

```bash
git add crates/synthia-guardian/tests/
git commit -m "test(guardian): add unit tests for GuardianDecision and CircuitBreaker"
```

---

## Task 9: Final Verification

- [ ] **Step 1: Run full test suite**

Run: `cargo test -p synthia-guardian -p synthia-agent 2>&1`
Expected: All tests PASS

- [ ] **Step 2: Run clippy**

Run: `cargo clippy -p synthia-guardian -p synthia-agent 2>&1 | head -50`
Expected: No warnings

- [ ] **Step 3: Final commit**

```bash
git add -A
git commit -m "feat(guardian): implement GAP-01 Guardian hybrid layer with circuit breaker"
```

---

**Plan complete and saved to `openspec/changes/gap-01-guardian-enhancement/plan.md`.**

Two execution options:

**1. Subagent-Driven (recommended)** - I dispatch a fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** - Execute tasks in this session using executing-plans, batch execution with checkpoints

Which approach?