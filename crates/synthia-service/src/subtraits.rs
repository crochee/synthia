//! Domain-specific service subtraits extending [`Service`].
//!
//! Each subtrait defines the domain API that the main loop and
//! tools consume. Concrete implementations live in their
//! respective crates; adapter types in `synthia-agent` bridge
//! them to both `Service` and the subtrait.

use async_trait::async_trait;

use crate::traits::{Service, ServiceError};

// ── SessionService (9.1) ──────────────────────────────────

/// Session management subtrait.
///
/// Wraps the session store operations that the main loop
/// needs: create, load, append, query, fork, compact,
/// rollback, snapshot.
#[async_trait]
pub trait SessionService: Service {
    /// Create a new session.
    async fn create(&self, session_id: &str) -> Result<(), ServiceError>;

    /// Load an existing session.
    async fn load(&self, session_id: &str) -> Result<(), ServiceError>;

    /// Append entries to a session.
    async fn append(
        &self,
        session_id: &str,
        entries: serde_json::Value,
    ) -> Result<(), ServiceError>;

    /// Query session state.
    async fn query(
        &self,
        session_id: &str,
    ) -> Result<serde_json::Value, ServiceError>;
}

// ── HookService (9.4) ────────────────────────────────────

/// Hook lifecycle subtrait.
///
/// Exposes the fire and register operations that the main
/// loop needs from the hook registry.
#[async_trait]
pub trait HookService: Service {
    /// Fire hooks at a given lifecycle point.
    async fn fire(
        &self,
        point: &str,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, ServiceError>;

    /// Register a new hook handler.
    async fn register_handler(
        &self,
        name: &str,
        config: serde_json::Value,
    ) -> Result<(), ServiceError>;
}

// ── PermissionService (9.7) ──────────────────────────────

/// Permission evaluation subtrait.
///
/// Wraps the merged policy evaluation and approval flow.
#[async_trait]
pub trait PermissionService: Service {
    /// Evaluate a permission request against the merged policy.
    async fn evaluate(&self, pattern: &str) -> PermissionDecision;

    /// Request interactive user approval.
    async fn request_approval(
        &self,
        request: PermissionRequest,
    ) -> Result<PermissionDecision, ServiceError>;

    /// Record a session-scoped permission rule.
    async fn record_session_rule(
        &self,
        pattern: &str,
        decision: PermissionDecision,
    ) -> Result<(), ServiceError>;

    /// Snapshot the current ruleset for stale detection.
    async fn snapshot_ruleset(&self) -> PermissionRulesetSnapshot;

    /// Evaluate through the doom-loop pipeline.
    ///
    /// Routes `GuardianService::detect()` findings through
    /// the permission policy before returning a decision.
    async fn evaluate_doom_loop(&self, context: &str) -> PermissionDecision;
}

/// Permission decision outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionDecision {
    Allow,
    Deny,
    Ask,
    /// Policy was stale — re-evaluation needed.
    PolicyStale {
        seen_generation: u64,
        current_generation: u64,
    },
}

/// Permission request for interactive approval.
#[derive(Debug, Clone)]
pub struct PermissionRequest {
    pub tool_name: String,
    pub pattern: String,
    pub reason: String,
}

/// Snapshot of the permission ruleset for stale detection.
#[derive(Debug, Clone)]
pub struct PermissionRulesetSnapshot {
    pub generation: u64,
    pub rule_count: usize,
}

// ── MemoryService (9.13) ─────────────────────────────────

/// Memory operations subtrait.
///
/// Wraps the hot/cold/episodic memory operations that the
/// main loop and tools consume.
#[async_trait]
pub trait MemoryServiceSub: Service {
    /// Store a memory event.
    async fn hot_store(
        &self,
        event: serde_json::Value,
    ) -> Result<(), ServiceError>;

    /// Retrieve from hot memory.
    async fn hot_get(
        &self,
        key: &str,
    ) -> Result<Option<serde_json::Value>, ServiceError>;

    /// Store to cold (persistent) memory.
    async fn cold_store(
        &self,
        key: &str,
        value: serde_json::Value,
    ) -> Result<(), ServiceError>;

    /// Search cold memory.
    async fn cold_search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<serde_json::Value>, ServiceError>;

    /// Run periodic consolidation.
    async fn consolidate(&self) -> Result<(), ServiceError>;
}
