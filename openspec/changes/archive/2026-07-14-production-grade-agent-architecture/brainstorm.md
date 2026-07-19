# Brainstorming: Production-Grade Agent Architecture

## Date: 2026-07-11

## Participants
- User (requester)
- Sisyphus (orchestrator)

## Background

Synthia has a solid ReAct loop foundation but critical architectural gaps vs production-grade agents (OpenCode, Codex, pi-mono). Analysis identified 9 capability gaps across P0/P1/P2 priorities.

## Scope Decision

**Selected: P0 + P1 (5 capabilities)**

| Priority | Capability | Gap Severity |
|----------|------------|--------------|
| P0 | Tool Cancellation Propagation | 🔴 Critical - broken chain |
| P1 | Async Permission Deferred | 🔴 High - blocking UX |
| P1 | Scoped Tool Registry | 🟡 High - no cleanup |
| P1 | Proactive Doom-Loop Detection | 🟡 High - reactive only |
| P1 | Smart Compaction Agent | 🟡 High - loses semantics |

## Key Decisions Made

### Decision 1: Tool Cancellation - Direct Parameter Addition (A1)

**Options Considered:**
- A1: Direct parameter addition to trait (breaking)
- A2: Adapter pattern for backward compat

**Decision: A1** - Direct addition

**Rationale:** Built-in tools are few. Direct parameter is cleaner. Adapter adds complexity without benefit.

```rust
// Before
async fn call_with_sandbox(&self, input: Value, sandbox: SandboxAttempt) -> Result<Value, ToolError>

// After
async fn call_with_sandbox(&self, input: Value, sandbox: SandboxAttempt, token: CancellationToken) -> Result<Value, ToolError>
```

### Decision 2: Doom-Loop - Dual System (B1)

**Options Considered:**
- B1: DoomLoopDetector alongside Guardian (complementary)
- B2: Replace Guardian's loop detection

**Decision: B1** - Dual system

**Rationale:**
- DoomLoopDetector: signature-based (tool + args), proactive
- GuardianCircuitBreaker: denial-count-based, reactive
- Different detection mechanisms, both useful

### Decision 3: Smart Compaction - Extend ContextAssembler (C1)

**Options Considered:**
- C1: Extend existing ContextAssembler
- C2: New SmartCompactionAgent standalone

**Decision: C1** - Extend existing

**Rationale:** ContextAssembler already has token budgeting logic. Replace truncation step with LLM summarization, reuse token selection logic.

## Design Summary

### 1. Tool Cancellation Propagation

**Problem:** ToolAdapter::execute() discards `_cancellation_token` (underscore = ignored).

**Solution:** Pass token through call chain.

```
AgentRunConfig.cancel_token
        ↓
orchestrator.execute(token)  ✓
        ↓
ToolAdapter::execute(token)  ← fix: pass not discard
        ↓
tool.call_with_sandbox(input, sandbox, token)
        ↓
yield points in long operations
```

**Files Changed:**
- `synthia-tool/src/traits.rs` - add token param
- `synthia-tool-orchestrator/src/lib.rs` - propagate token
- Built-in tools - add yield points

### 2. Async Permission Deferred

**Problem:** HeadlessApprovalService blocks agent thread while waiting.

**Solution:** PermissionFuture with oneshot channel.

```rust
pub struct PermissionFuture {
    rx: tokio::sync::oneshot::Receiver<PermissionResult>,
}

impl PermissionService for HeadlessApprovalService {
    fn ask(&self, req: PermissionRequest) -> PermissionFuture {
        // Immediately resolved with Denied
        PermissionFuture::immediate_denied()
    }
}

impl PermissionService for TuiApprovalService {
    fn ask(&self, req: PermissionRequest) -> PermissionFuture {
        // Show prompt, return future that resolves on user action
        let (tx, rx) = oneshot::channel();
        self.pending.insert(req.id, tx);
        PermissionFuture { rx }
    }
}
```

**Files Changed:**
- `synthia-permission/` - new PermissionFuture type
- `synthia-tool-orchestrator/src/lib.rs` - await future

### 3. Scoped Tool Registry

**Problem:** Global static registry, no per-session cleanup.

**Solution:** Token-based scoped registration with RAII guard.

```rust
pub struct ScopedToolRegistry {
    local: HashMap<String, Vec<ScopedRegistration>>,
    global: Arc<dyn ToolRegistry>,
}

pub struct ScopeGuard {
    token: Arc<()>,
    registry: Arc<StdRwLock<ScopedToolRegistry>>,
}

impl Drop for ScopeGuard {
    fn drop(&mut self) {
        // Remove all registrations with this token
    }
}

// Usage:
let (registry, guard) = ScopedToolRegistry::new(global);
registry.register_scoped(tools, guard.token());
// Session ends → guard dropped → auto cleanup
```

**Files Changed:**
- `synthia-tool/src/scoped_registry.rs` (new)

### 4. Proactive Doom-Loop Detection

**Problem:** Guardian circuit breaker only triggers after damage (reactive).

**Solution:** Sliding window of tool+args signatures, trigger at 3 identical.

```rust
pub struct DoomLoopDetector {
    recent: VecDeque<ToolCallSignature>,
    threshold: usize,  // 3
}

struct ToolCallSignature {
    tool_name: String,
    input_hash: u64,
}

impl DoomLoopDetector {
    pub fn check(&mut self, tool: &str, args: &Value) -> (LoopStatus, Option<LoopAction>) {
        let sig = ToolCallSignature { /* ... */ };

        // Sliding window check
        if self.recent.iter().take(self.threshold).all(|s| s == &sig) {
            return (LoopStatus::Detected, Some(LoopAction::RequirePermission));
        }

        self.recent.push_back(sig);
        if self.recent.len() > self.threshold {
            self.recent.pop_front();
        }
        (LoopStatus::Ok, None)
    }
}
```

**Files Changed:**
- `synthia-guardian/src/doom_loop_detector.rs` (new)

### 5. Smart Compaction Agent

**Problem:** Simple truncation loses semantic context.

**Solution:** Two-phase: backward token selection + LLM summarization.

```
PHASE 1: Token Selection
- Walk backward from most recent message
- Keep newest messages up to 8K tokens
- Split overflowing message: prefix → discarded, suffix → preserved

PHASE 2: LLM Summarization
- Same model as main agent, NO tools
- Prompt template: Goal/Progress/Decisions/Next Steps
- Output cap: 4096 tokens
- Incremental: include previous summary in prompt

PHASE 3: Insert Message
- { type: "compaction", text: summary, recent: preserved_tail }
```

**Files Changed:**
- `synthia-context/` - extend ContextAssembler

## Open Questions Resolved

1. **Breaking trait change?** Accepted - internal tool count is low, adapter pattern adds complexity
2. **Dual vs replace Guardian?** Dual - different detection mechanisms are complementary
3. **Extend vs new compaction?** Extend - reuse token budgeting logic

## Unresolved (Defer to Implementation)

1. Should `Tool::call_with_sandbox()` take `&CancellationToken` or clone?
2. Permission "always" persistence - file or DB?
3. Compaction summary in event log or session state?
4. Config keys for buffer/keep_tokens thresholds?

## Next Steps

1. Write design.md (this session)
2. Write proposal.md
3. Write specs for 5 capabilities
4. Write tasks for implementation
5. Execute with parallel agents
