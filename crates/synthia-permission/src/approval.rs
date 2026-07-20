use std::time::Duration;

use async_trait::async_trait;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

use crate::{permission_future::PermissionFuture, types::PermissionRequest};

/// Scope of an approval decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ApprovalPolicy {
    /// Approve a single matching call and remove the cached decision.
    Once,
    /// Approve every matching call for the remainder of the session.
    AlwaysForSession,
    /// Deny every matching call.
    Reject,
}

/// Outcome of an approval request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApprovalOutcome {
    Approve,
    Deny,
}

impl From<crate::level::Permission> for ApprovalOutcome {
    fn from(perm: crate::level::Permission) -> Self {
        match perm {
            crate::level::Permission::AutoApprove
            | crate::level::Permission::RequireConfirm
            | crate::level::Permission::RequireExplicit => {
                ApprovalOutcome::Approve
            }
            crate::level::Permission::Block
            | crate::level::Permission::Deny { .. } => ApprovalOutcome::Deny,
        }
    }
}

/// Errors that can be returned by an [`ApprovalService`].
///
/// Callers should treat every variant as a denial.
#[derive(
    Debug, thiserror::Error, Clone, PartialEq, Eq, Serialize, Deserialize,
)]
pub enum ApprovalError {
    #[error("approval request timed out")]
    Timeout,
    #[error("approval request was cancelled")]
    Cancelled,
    #[error("approval service unavailable")]
    Unavailable,
}

/// A deterministic scope used to cache approval decisions.
///
/// The scope key is derived solely from `tool_name` and normalized `args`;
/// it intentionally does not include the workspace root.
#[derive(Debug, Clone)]
pub struct ApprovalScope {
    pub tool_name: String,
    pub normalized_args: serde_json::Value,
}

impl ApprovalScope {
    /// Create a new scope from a tool name and raw arguments.
    pub fn new(tool_name: impl Into<String>, args: &serde_json::Value) -> Self {
        Self {
            tool_name: tool_name.into(),
            normalized_args: normalize(args),
        }
    }

    pub(crate) fn key(&self) -> ScopeKey {
        ScopeKey::new(&self.tool_name, &self.normalized_args)
    }
}

/// Identifier for a session-scoped approval context.
///
/// Pending approval requests are tracked per-session so that "always allow"
/// propagation and "reject" cascades can be scoped correctly. Two requests in
/// different sessions never interfere with each other.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionId(String);

impl SessionId {
    /// Create a new session identifier from any string-like value.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Return the underlying session identifier as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Result of an auto-resolved pending approval request.
///
/// When an "always allow" decision auto-resolves an identical pending request,
/// or when a "reject" decision cascade-terminates same-session pending
/// requests, the outcome (and optional cascade reason) is stored so the caller
/// can retrieve it via [`ApprovalStore::take_resolved`] instead of prompting
/// the user again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPending {
    /// The resolved outcome (`Approve` or `Deny`).
    pub outcome: ApprovalOutcome,
    /// Why the pending request was auto-resolved.
    /// - `None` for "always allow" auto-resolution of identical resources.
    /// - `Some("cascade-from-session-reject")` for reject cascade termination.
    pub cascade_reason: Option<String>,
}

/// Requests and tracks user/operator approval for sensitive tool calls.
#[async_trait]
pub trait ApprovalService: Send + Sync {
    /// Request approval for `tool` invoked with `args` under `policy`.
    ///
    /// `timeout` limits how long the service may wait for an answer.
    /// `cancel` allows the caller to abort the request.
    async fn request_approval(
        &self,
        tool: &str,
        args: &serde_json::Value,
        policy: ApprovalPolicy,
        timeout: Duration,
        cancel: CancellationToken,
    ) -> Result<ApprovalOutcome, ApprovalError>;

    /// Request approval asynchronously, returning a future that resolves to the
    /// permission outcome.
    ///
    /// This is the deferred variant: instead of blocking the caller, the
    /// returned [`PermissionFuture`] can be awaited while the agent continues
    /// processing other events.
    fn ask(&self, request: PermissionRequest) -> PermissionFuture;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ScopeKey([u8; 32]);

impl ScopeKey {
    fn new(tool: &str, normalized_args: &serde_json::Value) -> Self {
        let canonical_args = serde_json::to_string(normalized_args)
            .expect("JSON serialization is infallible");

        let mut hasher = Sha256::new();
        hasher.update(tool.as_bytes());
        hasher.update(canonical_args.as_bytes());
        Self(hasher.finalize().into())
    }
}

/// A pending approval request awaiting an interactive decision.
///
/// Stored in [`ApprovalStore::pending`] keyed by `request_id`. When an
/// "always allow" or "reject" decision is recorded via
/// [`ApprovalStore::set_with_session`], matching pending entries are moved to
/// the `resolved` map so the caller can retrieve them without prompting.
#[derive(Debug, Clone)]
pub(crate) struct PendingRequest {
    pub(crate) session_id: SessionId,
    pub(crate) tool_name: String,
    pub(crate) normalized_args: serde_json::Value,
}

/// In-memory cache of approval decisions keyed by a deterministic scope and
/// the policy under which the decision was made.
#[derive(Clone)]
pub struct ApprovalStore {
    decisions: DashMap<(ScopeKey, ApprovalPolicy), ApprovalOutcome>,
    pending: DashMap<String, PendingRequest>,
    resolved: DashMap<String, ResolvedPending>,
}

impl Default for ApprovalStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ApprovalStore {
    /// Create a new empty approval store.
    pub fn new() -> Self {
        Self {
            decisions: DashMap::new(),
            pending: DashMap::new(),
            resolved: DashMap::new(),
        }
    }

    /// Look up a cached decision for `scope` under `policy`.
    ///
    /// - `Once` decisions are consumed by the lookup and removed from the cache.
    /// - `AlwaysForSession` decisions persist until explicitly overwritten.
    /// - `Reject` decisions always yield `ApprovalOutcome::Deny` when present.
    pub fn get(
        &self,
        scope: &ApprovalScope,
        policy: ApprovalPolicy,
    ) -> Option<ApprovalOutcome> {
        let key = (scope.key(), policy);
        match policy {
            ApprovalPolicy::Once => {
                self.decisions.remove(&key).map(|(_, outcome)| outcome)
            }
            ApprovalPolicy::AlwaysForSession => {
                self.decisions.get(&key).map(|entry| *entry)
            }
            ApprovalPolicy::Reject => {
                if self.decisions.contains_key(&key) {
                    Some(ApprovalOutcome::Deny)
                } else {
                    None
                }
            }
        }
    }

    /// Cache a decision for `scope` under `policy`.
    pub fn set(
        &self,
        scope: &ApprovalScope,
        outcome: ApprovalOutcome,
        policy: ApprovalPolicy,
    ) {
        self.decisions.insert((scope.key(), policy), outcome);
    }

    /// Register a pending approval request so it can be auto-resolved by a
    /// subsequent "always allow" decision or cascade-terminated by a "reject"
    /// decision in the same session.
    ///
    /// `args` are normalized via [`normalize`] before being stored, so the
    /// derived resource key matches the one computed by
    /// [`ApprovalScope::new`].
    pub fn register_pending(
        &self,
        session_id: SessionId,
        request_id: String,
        tool_name: &str,
        args: &serde_json::Value,
    ) {
        let normalized_args = normalize(args);
        self.pending.insert(
            request_id,
            PendingRequest {
                session_id,
                tool_name: tool_name.to_string(),
                normalized_args,
            },
        );
    }

    /// Take the auto-resolved outcome for a pending request, if any.
    ///
    /// Returns `Some(ResolvedPending)` if the request was auto-resolved by an
    /// "always allow" propagation or cascade-terminated by a "reject" in the
    /// same session. Returns `None` if the request is still pending (or was
    /// never registered), in which case the caller should prompt the user.
    ///
    /// The resolved entry is removed from the store on retrieval.
    pub fn take_resolved(&self, request_id: &str) -> Option<ResolvedPending> {
        self.resolved.remove(request_id).map(|(_, v)| v)
    }

    /// Cache a decision for `scope` under `policy` and propagate to pending
    /// requests in `session_id`:
    ///
    /// - `AlwaysForSession` + `Approve`: auto-resolves pending requests in the
    ///   same session whose resources are IDENTICAL to `scope` (same tool +
    ///   same normalized args). Overlapping but non-identical pending requests
    ///   are left untouched and will still prompt the user.
    /// - `Reject` + `Deny`: cascade-terminates ALL pending requests in the same
    ///   session with `cascade_reason = "cascade-from-session-reject"`.
    ///
    /// Pending requests in OTHER sessions are never affected.
    ///
    /// This is a superset of [`ApprovalStore::set`]; callers that do not need
    /// pending propagation should continue to use `set` directly.
    pub fn set_with_session(
        &self,
        session_id: &SessionId,
        scope: &ApprovalScope,
        outcome: ApprovalOutcome,
        policy: ApprovalPolicy,
    ) {
        self.set(scope, outcome, policy);
        self.propagate_to_pending(session_id, scope, outcome, policy);
    }

    fn propagate_to_pending(
        &self,
        session_id: &SessionId,
        scope: &ApprovalScope,
        outcome: ApprovalOutcome,
        policy: ApprovalPolicy,
    ) {
        let target_key = scope.key();
        let cascade_reason = match (policy, outcome) {
            (ApprovalPolicy::AlwaysForSession, ApprovalOutcome::Approve) => {
                None
            }
            (ApprovalPolicy::Reject, ApprovalOutcome::Deny) => {
                Some("cascade-from-session-reject".to_string())
            }
            _ => return,
        };

        // Collect matching request_ids first to avoid mutating while iterating.
        let to_resolve: Vec<String> = self
            .pending
            .iter()
            .filter(|entry| entry.value().session_id == *session_id)
            .filter_map(|entry| {
                let req = entry.value();
                let matches_scope = match policy {
                    ApprovalPolicy::AlwaysForSession => {
                        // Only auto-resolve IDENTICAL resources (same tool +
                        // same normalized args). Overlapping but non-identical
                        // resources must still prompt the user.
                        let pending_scope = ApprovalScope {
                            tool_name: req.tool_name.clone(),
                            normalized_args: req.normalized_args.clone(),
                        };
                        pending_scope.key() == target_key
                    }
                    ApprovalPolicy::Reject => true,
                    ApprovalPolicy::Once => false,
                };
                if matches_scope {
                    Some(entry.key().clone())
                } else {
                    None
                }
            })
            .collect();

        for request_id in to_resolve {
            let _ = self.pending.remove(&request_id);
            self.resolved.insert(
                request_id,
                ResolvedPending {
                    outcome,
                    cascade_reason: cascade_reason.clone(),
                },
            );
        }
    }
}

/// An approval service that never asks for interaction and always denies.
pub struct HeadlessApprovalService;

impl Default for HeadlessApprovalService {
    fn default() -> Self {
        Self
    }
}

#[async_trait]
impl ApprovalService for HeadlessApprovalService {
    async fn request_approval(
        &self,
        _tool: &str,
        _args: &serde_json::Value,
        policy: ApprovalPolicy,
        _timeout: Duration,
        cancel: CancellationToken,
    ) -> Result<ApprovalOutcome, ApprovalError> {
        if cancel.is_cancelled() {
            return Err(ApprovalError::Cancelled);
        }
        match policy {
            ApprovalPolicy::Reject => Ok(ApprovalOutcome::Deny),
            ApprovalPolicy::Once | ApprovalPolicy::AlwaysForSession => {
                Err(ApprovalError::Unavailable)
            }
        }
    }

    fn ask(&self, _request: PermissionRequest) -> PermissionFuture {
        PermissionFuture::immediate_denied()
    }
}

/// Skeleton for an approval service that can query an [`ApprovalStore`] and,
/// on cache miss, delegate to an interactive UI (implemented in a later task).
pub struct InteractiveApprovalService {
    store: ApprovalStore,
}

impl InteractiveApprovalService {
    /// Create a new interactive approval service backed by `store`.
    pub fn new(store: ApprovalStore) -> Self {
        Self { store }
    }
}

#[async_trait]
impl ApprovalService for InteractiveApprovalService {
    async fn request_approval(
        &self,
        tool: &str,
        args: &serde_json::Value,
        policy: ApprovalPolicy,
        _timeout: Duration,
        cancel: CancellationToken,
    ) -> Result<ApprovalOutcome, ApprovalError> {
        if cancel.is_cancelled() {
            return Err(ApprovalError::Cancelled);
        }

        let scope = ApprovalScope::new(tool, args);
        if let Some(outcome) = self.store.get(&scope, policy) {
            return Ok(outcome);
        }

        // Cache miss: a real implementation would wait for interactive UI input.
        // Returning `Unavailable` signals that UI interaction is required.
        Err(ApprovalError::Unavailable)
    }

    fn ask(&self, _request: PermissionRequest) -> PermissionFuture {
        PermissionFuture::immediate_denied()
    }
}

/// Recursively sort object keys so the serialized form is deterministic.
///
/// Exposed as `pub(crate)` so that `PermissionChecker::check` and
/// `TerminalApprovalService::record_always_decision` derive the same
/// resource key from a `serde_json::Value` regardless of key insertion
/// order. This mirrors `ApprovalScope`'s normalization strategy and
/// ensures `saved_rules` matching is order-independent.
pub(crate) fn normalize(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut entries: Vec<_> = map.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            let mut normalized = serde_json::Map::with_capacity(entries.len());
            for (k, v) in entries {
                normalized.insert(k.clone(), normalize(v));
            }
            serde_json::Value::Object(normalized)
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(normalize).collect())
        }
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio_util::sync::CancellationToken;

    use super::*;

    #[test]
    fn approval_store_cache_hit() {
        let store = ApprovalStore::new();
        let scope = ApprovalScope::new(
            "write",
            &serde_json::json!({ "path": "foo.txt", "content": "hi" }),
        );

        assert!(store.get(&scope, ApprovalPolicy::Once).is_none());
        store.set(&scope, ApprovalOutcome::Approve, ApprovalPolicy::Once);
        assert_eq!(
            store.get(&scope, ApprovalPolicy::Once),
            Some(ApprovalOutcome::Approve)
        );
    }

    #[test]
    fn approval_store_cache_miss() {
        let store = ApprovalStore::new();
        let scope = ApprovalScope::new(
            "read",
            &serde_json::json!({ "path": "bar.txt" }),
        );

        assert!(store.get(&scope, ApprovalPolicy::Once).is_none());
    }

    #[test]
    fn approval_store_scope_key_ignores_workspace() {
        let store = ApprovalStore::new();
        let args = serde_json::json!({ "path": "baz.txt" });
        let scope_a = ApprovalScope::new("read", &args);
        let scope_b = ApprovalScope::new("read", &args);

        store.set(&scope_a, ApprovalOutcome::Approve, ApprovalPolicy::Once);
        assert_eq!(
            store.get(&scope_b, ApprovalPolicy::Once),
            Some(ApprovalOutcome::Approve)
        );
    }

    #[test]
    fn approval_store_once_is_consumed_after_one_lookup() {
        let store = ApprovalStore::new();
        let scope =
            ApprovalScope::new("read", &serde_json::json!({ "path": "x.txt" }));

        store.set(&scope, ApprovalOutcome::Approve, ApprovalPolicy::Once);
        assert_eq!(
            store.get(&scope, ApprovalPolicy::Once),
            Some(ApprovalOutcome::Approve)
        );
        assert!(store.get(&scope, ApprovalPolicy::Once).is_none());
    }

    #[test]
    fn approval_store_always_for_session_persists_across_lookups() {
        let store = ApprovalStore::new();
        let scope =
            ApprovalScope::new("read", &serde_json::json!({ "path": "y.txt" }));

        store.set(
            &scope,
            ApprovalOutcome::Approve,
            ApprovalPolicy::AlwaysForSession,
        );
        assert_eq!(
            store.get(&scope, ApprovalPolicy::AlwaysForSession),
            Some(ApprovalOutcome::Approve)
        );
        assert_eq!(
            store.get(&scope, ApprovalPolicy::AlwaysForSession),
            Some(ApprovalOutcome::Approve)
        );
        // A different policy for the same scope should still miss.
        assert!(store.get(&scope, ApprovalPolicy::Once).is_none());
    }

    #[test]
    fn approval_store_reject_persists_as_deny() {
        let store = ApprovalStore::new();
        let scope = ApprovalScope::new(
            "write",
            &serde_json::json!({ "path": "z.txt" }),
        );

        store.set(&scope, ApprovalOutcome::Deny, ApprovalPolicy::Reject);
        assert_eq!(
            store.get(&scope, ApprovalPolicy::Reject),
            Some(ApprovalOutcome::Deny)
        );
        assert!(store.get(&scope, ApprovalPolicy::Once).is_none());
    }

    #[tokio::test]
    async fn headless_approval_service_returns_unavailable_for_once() {
        let service = HeadlessApprovalService;
        let outcome = service
            .request_approval(
                "write",
                &serde_json::json!({}),
                ApprovalPolicy::Once,
                Duration::from_secs(1),
                CancellationToken::new(),
            )
            .await;

        assert_eq!(outcome, Err(ApprovalError::Unavailable));
    }

    #[tokio::test]
    async fn headless_approval_service_returns_deny_for_reject() {
        let service = HeadlessApprovalService;
        let outcome = service
            .request_approval(
                "write",
                &serde_json::json!({}),
                ApprovalPolicy::Reject,
                Duration::from_secs(1),
                CancellationToken::new(),
            )
            .await;

        assert_eq!(outcome, Ok(ApprovalOutcome::Deny));
    }

    #[tokio::test]
    async fn headless_approval_service_respects_cancellation() {
        let service = HeadlessApprovalService;
        let cancel = CancellationToken::new();
        cancel.cancel();

        let outcome = service
            .request_approval(
                "write",
                &serde_json::json!({}),
                ApprovalPolicy::Once,
                Duration::from_secs(1),
                cancel,
            )
            .await;

        assert_eq!(outcome, Err(ApprovalError::Cancelled));
    }

    #[tokio::test]
    async fn interactive_approval_service_returns_cached_outcome() {
        let store = ApprovalStore::new();
        let scope =
            ApprovalScope::new("read", &serde_json::json!({ "path": "x.txt" }));
        store.set(
            &scope,
            ApprovalOutcome::Approve,
            ApprovalPolicy::AlwaysForSession,
        );

        let service = InteractiveApprovalService::new(store);
        let outcome = service
            .request_approval(
                "read",
                &serde_json::json!({ "path": "x.txt" }),
                ApprovalPolicy::AlwaysForSession,
                Duration::from_secs(1),
                CancellationToken::new(),
            )
            .await;

        assert_eq!(outcome, Ok(ApprovalOutcome::Approve));
    }

    #[tokio::test]
    async fn interactive_approval_service_unavailable_on_cache_miss() {
        let service = InteractiveApprovalService::new(ApprovalStore::new());
        let outcome = service
            .request_approval(
                "read",
                &serde_json::json!({ "path": "y.txt" }),
                ApprovalPolicy::Once,
                Duration::from_secs(1),
                CancellationToken::new(),
            )
            .await;

        assert_eq!(outcome, Err(ApprovalError::Unavailable));
    }

    // ---- 3.1: "always" propagation + "reject" cascade ----

    #[test]
    fn always_allow_auto_resolves_identical_pending() {
        let store = ApprovalStore::new();
        let session = SessionId::new("session-A");
        let args = serde_json::json!(["ls"]);

        // Two pending requests with identical resources.
        store.register_pending(
            session.clone(),
            "req-1".to_string(),
            "bash",
            &args,
        );
        store.register_pending(
            session.clone(),
            "req-2".to_string(),
            "bash",
            &args,
        );

        // User "always allows" req-1.
        let scope = ApprovalScope::new("bash", &args);
        store.set_with_session(
            &session,
            &scope,
            ApprovalOutcome::Approve,
            ApprovalPolicy::AlwaysForSession,
        );

        // req-2 should be auto-resolved as Approve (no prompt).
        let resolved = store
            .take_resolved("req-2")
            .expect("req-2 should be auto-resolved");
        assert_eq!(resolved.outcome, ApprovalOutcome::Approve);
        assert!(resolved.cascade_reason.is_none());
    }

    #[test]
    fn always_allow_does_not_resolve_overlapping() {
        let store = ApprovalStore::new();
        let session = SessionId::new("session-A");

        // Pending request has ["ls", "pwd"].
        let pending_args = serde_json::json!(["ls", "pwd"]);
        store.register_pending(
            session.clone(),
            "req-2".to_string(),
            "bash",
            &pending_args,
        );

        // User "always allows" ["ls"] — overlapping but NOT identical.
        let allowed_args = serde_json::json!(["ls"]);
        let scope = ApprovalScope::new("bash", &allowed_args);
        store.set_with_session(
            &session,
            &scope,
            ApprovalOutcome::Approve,
            ApprovalPolicy::AlwaysForSession,
        );

        // req-2 should NOT be auto-resolved (still needs a prompt).
        assert!(store.take_resolved("req-2").is_none());
    }

    #[test]
    fn cross_session_isolation() {
        let store = ApprovalStore::new();
        let session_a = SessionId::new("session-A");
        let session_b = SessionId::new("session-B");
        let args = serde_json::json!(["ls"]);

        // Pending request in session B.
        store.register_pending(
            session_b.clone(),
            "req-B".to_string(),
            "bash",
            &args,
        );

        // User "always allows" in session A.
        let scope = ApprovalScope::new("bash", &args);
        store.set_with_session(
            &session_a,
            &scope,
            ApprovalOutcome::Approve,
            ApprovalPolicy::AlwaysForSession,
        );

        // Session B's pending should NOT be auto-resolved.
        assert!(store.take_resolved("req-B").is_none());
    }

    #[test]
    fn reject_cascades_to_same_session_pending() {
        let store = ApprovalStore::new();
        let session = SessionId::new("session-A");

        // Three pending requests in session A.
        store.register_pending(
            session.clone(),
            "req-1".to_string(),
            "bash",
            &serde_json::json!(["ls"]),
        );
        store.register_pending(
            session.clone(),
            "req-2".to_string(),
            "bash",
            &serde_json::json!(["pwd"]),
        );
        store.register_pending(
            session.clone(),
            "req-3".to_string(),
            "write",
            &serde_json::json!({"path": "foo.txt"}),
        );

        // User rejects req-1.
        let scope = ApprovalScope::new("bash", &serde_json::json!(["ls"]));
        store.set_with_session(
            &session,
            &scope,
            ApprovalOutcome::Deny,
            ApprovalPolicy::Reject,
        );

        // All three should be terminated with cascade-from-session-reject.
        for req_id in ["req-1", "req-2", "req-3"] {
            let resolved = store.take_resolved(req_id).unwrap_or_else(|| {
                panic!("{req_id} should be cascade-resolved")
            });
            assert_eq!(resolved.outcome, ApprovalOutcome::Deny);
            assert_eq!(
                resolved.cascade_reason.as_deref(),
                Some("cascade-from-session-reject")
            );
        }
    }

    #[test]
    fn reject_does_not_cross_session() {
        let store = ApprovalStore::new();
        let session_a = SessionId::new("session-A");
        let session_b = SessionId::new("session-B");
        let args = serde_json::json!(["ls"]);

        // Pending in both sessions with identical resources.
        store.register_pending(
            session_a.clone(),
            "req-A".to_string(),
            "bash",
            &args,
        );
        store.register_pending(
            session_b.clone(),
            "req-B".to_string(),
            "bash",
            &args,
        );

        // User rejects in session A.
        let scope = ApprovalScope::new("bash", &args);
        store.set_with_session(
            &session_a,
            &scope,
            ApprovalOutcome::Deny,
            ApprovalPolicy::Reject,
        );

        // req-A should be cascade-resolved.
        let resolved_a = store
            .take_resolved("req-A")
            .expect("req-A should be cascade-resolved");
        assert_eq!(resolved_a.outcome, ApprovalOutcome::Deny);
        assert_eq!(
            resolved_a.cascade_reason.as_deref(),
            Some("cascade-from-session-reject")
        );

        // req-B in session B should NOT be affected.
        assert!(store.take_resolved("req-B").is_none());
    }
}
