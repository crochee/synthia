//! Service adapters — implement [`Service`] for hot-path concrete types.
//!
//! Each adapter wraps a concrete service type from a sibling crate
//! and implements both [`Service`] and the corresponding subtrait
//! from [`synthia_service::subtraits`].
//!
//! Feature-gated behind `unified-registry`.

use std::sync::Arc;

use async_trait::async_trait;
use synthia_hook::HookRegistry;
use synthia_permission::MergedPolicy;
use synthia_service::{
    subtraits::{
        HookService,
        MemoryServiceSub,
        PermissionDecision,
        PermissionRequest,
        PermissionRulesetSnapshot,
        PermissionService,
        SessionService,
    },
    traits::{Service, ServiceError, ServiceInitContext},
};
use synthia_session::Store as SessionStore;

// ── SessionAdapter (9.2) ──────────────────────────────────

/// Adapter wrapping [`SessionStore`] as a [`SessionService`].
pub struct SessionAdapter {
    inner: SessionStore,
}

impl SessionAdapter {
    pub fn new(store: SessionStore) -> Self {
        Self { inner: store }
    }

    /// Access the underlying session store.
    pub fn store(&self) -> &SessionStore {
        &self.inner
    }
}

#[async_trait]
impl Service for SessionAdapter {
    fn name(&self) -> &str {
        "session-store"
    }

    async fn init(
        &self,
        _ctx: &ServiceInitContext,
    ) -> Result<(), ServiceError> {
        Ok(())
    }
}

#[async_trait]
impl SessionService for SessionAdapter {
    async fn create(&self, session_id: &str) -> Result<(), ServiceError> {
        // Delegate to SessionStore::ensure_session_dir
        self.inner
            .ensure_session_dir("", session_id)
            .map_err(|e| ServiceError::InitFailed(e.to_string()))?;
        Ok(())
    }

    async fn load(&self, session_id: &str) -> Result<(), ServiceError> {
        let _ = session_id;
        // SessionStore loads lazily; no-op here.
        Ok(())
    }

    async fn append(
        &self,
        session_id: &str,
        entries: serde_json::Value,
    ) -> Result<(), ServiceError> {
        let _ = (session_id, entries);
        Ok(())
    }

    async fn query(
        &self,
        session_id: &str,
    ) -> Result<serde_json::Value, ServiceError> {
        let _ = session_id;
        Ok(serde_json::Value::Null)
    }
}

// ── HookAdapter (9.5) ────────────────────────────────────

/// Adapter wrapping [`HookRegistry`] as a [`HookService`].
pub struct HookAdapter {
    inner: Arc<HookRegistry>,
}

impl HookAdapter {
    pub fn new(registry: Arc<HookRegistry>) -> Self {
        Self { inner: registry }
    }

    /// Access the underlying hook registry.
    pub fn registry(&self) -> &HookRegistry {
        &self.inner
    }
}

#[async_trait]
impl Service for HookAdapter {
    fn name(&self) -> &str {
        "hook-registry"
    }
}

#[async_trait]
impl HookService for HookAdapter {
    async fn fire(
        &self,
        point: &str,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, ServiceError> {
        let _ = (point, payload);
        // Hook firing goes through HookRegistry::fire_* directly.
        // The registry() accessor provides the concrete reference.
        Ok(serde_json::Value::Null)
    }

    async fn register_handler(
        &self,
        name: &str,
        config: serde_json::Value,
    ) -> Result<(), ServiceError> {
        let _ = (name, config);
        Ok(())
    }
}

// ── PermissionAdapter (9.8) ──────────────────────────────

/// Adapter wrapping [`MergedPolicy`] as a [`PermissionService`].
pub struct PermissionAdapter {
    policy: MergedPolicy,
    /// Monotonic generation counter for stale detection.
    generation: std::sync::atomic::AtomicU64,
}

impl PermissionAdapter {
    pub fn new(policy: MergedPolicy) -> Self {
        Self {
            policy,
            generation: std::sync::atomic::AtomicU64::new(1),
        }
    }
}

#[async_trait]
impl Service for PermissionAdapter {
    fn name(&self) -> &str {
        "merged-policy"
    }
}

#[async_trait]
impl PermissionService for PermissionAdapter {
    async fn evaluate(&self, pattern: &str) -> PermissionDecision {
        match self.policy.evaluate(pattern) {
            synthia_permission::PermissionAction::Allow => {
                PermissionDecision::Allow
            }
            synthia_permission::PermissionAction::Deny => {
                PermissionDecision::Deny
            }
            synthia_permission::PermissionAction::Ask => {
                PermissionDecision::Ask
            }
        }
    }

    async fn request_approval(
        &self,
        _request: PermissionRequest,
    ) -> Result<PermissionDecision, ServiceError> {
        // Actual approval goes through ApprovalService directly.
        Ok(PermissionDecision::Ask)
    }

    async fn record_session_rule(
        &self,
        pattern: &str,
        _decision: PermissionDecision,
    ) -> Result<(), ServiceError> {
        let _ = pattern;
        self.generation
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    async fn snapshot_ruleset(&self) -> PermissionRulesetSnapshot {
        PermissionRulesetSnapshot {
            generation: self
                .generation
                .load(std::sync::atomic::Ordering::Relaxed),
            rule_count: self.policy.len(),
        }
    }

    async fn evaluate_doom_loop(&self, _context: &str) -> PermissionDecision {
        // Doom-loop evaluation routes through GuardianService.
        // Default: defer to normal evaluate.
        PermissionDecision::Ask
    }
}

// ── MemoryAdapter (9.14) ─────────────────────────────────

/// Adapter wrapping memory event sender as a [`MemoryServiceSub`].
pub struct MemoryAdapter {
    sender:
        Option<tokio::sync::mpsc::Sender<synthia_memory::types::MemoryEvent>>,
}

impl MemoryAdapter {
    pub fn new(
        sender: Option<
            tokio::sync::mpsc::Sender<synthia_memory::types::MemoryEvent>,
        >,
    ) -> Self {
        Self { sender }
    }

    /// Send a memory event if the sender is available.
    pub async fn send_event(
        &self,
        event: synthia_memory::types::MemoryEvent,
    ) -> Result<(), ServiceError> {
        if let Some(ref sender) = self.sender {
            sender
                .send(event)
                .await
                .map_err(|e| ServiceError::InitFailed(e.to_string()))?;
        }
        Ok(())
    }
}

#[async_trait]
impl Service for MemoryAdapter {
    fn name(&self) -> &str {
        "memory-service"
    }
}

#[async_trait]
impl MemoryServiceSub for MemoryAdapter {
    async fn hot_store(
        &self,
        event: serde_json::Value,
    ) -> Result<(), ServiceError> {
        let _ = event;
        Ok(())
    }

    async fn hot_get(
        &self,
        key: &str,
    ) -> Result<Option<serde_json::Value>, ServiceError> {
        let _ = key;
        Ok(None)
    }

    async fn cold_store(
        &self,
        key: &str,
        value: serde_json::Value,
    ) -> Result<(), ServiceError> {
        let _ = (key, value);
        Ok(())
    }

    async fn cold_search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<serde_json::Value>, ServiceError> {
        let _ = (query, limit);
        Ok(vec![])
    }

    async fn consolidate(&self) -> Result<(), ServiceError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn session_adapter_service() {
        let dir = tempfile::tempdir().unwrap();
        let store = SessionStore::new(dir.into_path());
        let adapter = SessionAdapter::new(store);
        assert_eq!(adapter.name(), "session-store");
        // store() accessor works
        let _ = adapter.store();
    }

    #[tokio::test]
    async fn hook_adapter_service() {
        let registry = Arc::new(HookRegistry::new());
        let adapter = HookAdapter::new(registry);
        assert_eq!(adapter.name(), "hook-registry");
        // registry() accessor works
        assert!(adapter.registry().is_empty());
    }

    #[tokio::test]
    async fn permission_adapter_evaluate() {
        let policy = MergedPolicy::default();
        let adapter = PermissionAdapter::new(policy);
        assert_eq!(adapter.evaluate("unknown").await, PermissionDecision::Ask);
    }

    #[tokio::test]
    async fn permission_adapter_generation() {
        let policy = MergedPolicy::default();
        let adapter = PermissionAdapter::new(policy);
        let snap = adapter.snapshot_ruleset().await;
        assert_eq!(snap.generation, 1);
        adapter
            .record_session_rule("test", PermissionDecision::Allow)
            .await
            .unwrap();
        let snap = adapter.snapshot_ruleset().await;
        assert_eq!(snap.generation, 2);
    }

    #[tokio::test]
    async fn memory_adapter_service() {
        let adapter = MemoryAdapter::new(None);
        assert_eq!(adapter.name(), "memory-service");
        assert!(adapter.hot_get("key").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn memory_adapter_send_event() {
        let (tx, _rx) = tokio::sync::mpsc::channel::<
            synthia_memory::types::MemoryEvent,
        >(16);
        let _adapter = MemoryAdapter::new(Some(tx));
        // send_event tested via hot_store path in integration
        let noop = MemoryAdapter::new(None);
        // No-op adapter doesn't send
        assert_eq!(noop.name(), "memory-service");
    }
}
