use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

use crate::{
    checker::PermissionChecker,
    permission_future::PermissionFuture,
    types::PermissionRequest,
};

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

    fn key(&self) -> ScopeKey {
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
struct ScopeKey([u8; 32]);

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
struct PendingRequest {
    session_id: SessionId,
    tool_name: String,
    normalized_args: serde_json::Value,
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

/// An approval service that prompts the user on the terminal and records the
/// decision in an [`ApprovalStore`].
///
/// The prompt accepts the following responses:
/// - `y`, `yes` -> approve once
/// - `n`, `no`, `reject` -> deny and cache the rejection
/// - `a`, `always` -> approve for the remainder of the session
///
/// When constructed via [`TerminalApprovalService::new_with_checker`], an
/// "always" reply is also forwarded to the bundled [`PermissionChecker`] via
/// `remember_always`, so subsequent `PermissionChecker::check` calls for the
/// same `(tool, args)` pair return `Permission::AutoApprove` without
/// prompting again. The legacy [`TerminalApprovalService::new`] constructor
/// keeps `checker = None` for backward compatibility (the approval store's
/// own `AlwaysForSession` cache still works for repeat lookups within the
/// same process).
#[derive(Clone)]
pub struct TerminalApprovalService {
    store: ApprovalStore,
    checker: Option<Arc<PermissionChecker>>,
}

impl TerminalApprovalService {
    /// Create a new terminal approval service backed by `store` and no
    /// `PermissionChecker`. This preserves the pre-F19 behavior: "always"
    /// replies are cached in the [`ApprovalStore`] but not propagated to
    /// any `PermissionChecker`.
    pub fn new(store: ApprovalStore) -> Self {
        Self {
            store,
            checker: None,
        }
    }

    /// Create a new terminal approval service backed by `store` and
    /// `checker`. When the user replies "always" / "a" /
    /// "always-for-session", `checker.remember_always(tool, args)` is
    /// invoked in addition to caching the decision in `store`. The
    /// `PermissionChecker` (and any clones sharing its `saved_rules`)
    /// will then auto-approve matching requests for the remainder of
    /// the session.
    ///
    /// **Production wiring note**: The CLI
    /// (`synthia-cli/src/repl_core/repl/agent_message.rs`) currently
    /// uses [`TerminalApprovalService::new`] (without checker) because
    /// the CLI context does not yet expose a `PermissionChecker`
    /// handle. Once the CLI wiring exposes the checker, switch to
    /// `new_with_checker` to enable F19 end-to-end.
    pub fn new_with_checker(
        store: ApprovalStore,
        checker: Arc<PermissionChecker>,
    ) -> Self {
        Self {
            store,
            checker: Some(checker),
        }
    }

    /// Record an "always" approval decision to both the [`ApprovalStore`]
    /// (for prompt suppression) and the bundled [`PermissionChecker`] (for
    /// `check()` short-circuit). Extracted from `request_approval` so
    /// the F19 wiring can be unit-tested without mocking stdin.
    ///
    /// `tool` and `args` mirror the parameters passed to
    /// [`Self::request_approval`]. The `args` value is normalized via
    /// [`normalize`] before being forwarded to
    /// [`PermissionChecker::remember_always`], so the derived resource
    /// key matches the one computed by
    /// [`PermissionChecker::check`](crate::checker::PermissionChecker::check).
    pub(super) fn record_always_decision(
        &self,
        tool: &str,
        args: &serde_json::Value,
    ) {
        let scope = ApprovalScope::new(tool, args);
        self.store.set(
            &scope,
            ApprovalOutcome::Approve,
            ApprovalPolicy::AlwaysForSession,
        );
        if let Some(checker) = &self.checker {
            checker
                .remember_always(tool.to_string(), normalize(args).to_string());
        }
    }
}

impl TerminalApprovalService {
    /// Internal helper that accepts arbitrary async reader/writer pairs so
    /// the interactive prompt can be unit-tested without touching real
    /// stdin/stdout.
    ///
    /// The parameter count is intentionally high: this is the single internal
    /// entry point shared by the real `ApprovalService` implementation and the
    /// test harness. Splitting it would not improve readability.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn request_approval_with_io<R, W>(
        &self,
        tool: &str,
        args: &serde_json::Value,
        policy: ApprovalPolicy,
        timeout: Duration,
        cancel: CancellationToken,
        reader: R,
        mut writer: W,
    ) -> Result<ApprovalOutcome, ApprovalError>
    where
        R: AsyncBufReadExt + Unpin,
        W: AsyncWriteExt + Unpin,
    {
        if cancel.is_cancelled() {
            return Err(ApprovalError::Cancelled);
        }

        let scope = ApprovalScope::new(tool, args);
        if let Some(outcome) = self.store.get(&scope, policy) {
            return Ok(outcome);
        }

        let prompt = format!(
            "Approve tool call?\n  tool: {tool}\n  args: {args}\n[y/n/a/always/reject]: ",
        );
        if writer.write_all(prompt.as_bytes()).await.is_err() {
            return Err(ApprovalError::Unavailable);
        }
        if writer.flush().await.is_err() {
            return Err(ApprovalError::Unavailable);
        }

        let mut lines = reader.lines();
        let line = tokio::select! {
            _ = cancel.cancelled() => return Err(ApprovalError::Cancelled),
            _ = tokio::time::sleep(timeout) => return Err(ApprovalError::Timeout),
            result = lines.next_line() => result.map_err(|_| ApprovalError::Unavailable)?,
        };

        let line = line.ok_or(ApprovalError::Timeout)?;
        let outcome = match line.trim().to_ascii_lowercase().as_str() {
            "y" | "yes" => {
                self.store.set(
                    &scope,
                    ApprovalOutcome::Approve,
                    ApprovalPolicy::Once,
                );
                ApprovalOutcome::Approve
            }
            "a" | "always" | "always-for-session" => {
                self.record_always_decision(tool, args);
                ApprovalOutcome::Approve
            }
            "n" | "no" | "reject" => {
                self.store.set(
                    &scope,
                    ApprovalOutcome::Deny,
                    ApprovalPolicy::Reject,
                );
                ApprovalOutcome::Deny
            }
            _ => ApprovalOutcome::Deny,
        };

        Ok(outcome)
    }
}

#[async_trait]
impl ApprovalService for TerminalApprovalService {
    async fn request_approval(
        &self,
        tool: &str,
        args: &serde_json::Value,
        policy: ApprovalPolicy,
        timeout: Duration,
        cancel: CancellationToken,
    ) -> Result<ApprovalOutcome, ApprovalError> {
        let stdin = tokio::io::BufReader::new(tokio::io::stdin());
        let stdout = tokio::io::stdout();
        self.request_approval_with_io(
            tool, args, policy, timeout, cancel, stdin, stdout,
        )
        .await
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

    #[tokio::test]
    async fn terminal_approval_service_uses_cached_decision_without_prompt() {
        let store = ApprovalStore::new();
        let scope = ApprovalScope::new(
            "write",
            &serde_json::json!({ "path": "cached.txt" }),
        );
        store.set(
            &scope,
            ApprovalOutcome::Approve,
            ApprovalPolicy::AlwaysForSession,
        );

        let service = TerminalApprovalService::new(store);
        let outcome = service
            .request_approval(
                "write",
                &serde_json::json!({ "path": "cached.txt" }),
                ApprovalPolicy::AlwaysForSession,
                Duration::from_secs(1),
                CancellationToken::new(),
            )
            .await;

        assert_eq!(outcome, Ok(ApprovalOutcome::Approve));
    }

    // ---- F19: TerminalApprovalService <-> PermissionChecker wiring ----

    #[tokio::test]
    async fn test_terminal_approval_records_always_to_checker() {
        use crate::{
            level::Permission,
            merged_policy::MergedPolicy,
            rule::{PermissionAction, PermissionRule},
            types::PermissionRequest,
        };

        // Create a checker where `bash` is normally RequireConfirm
        // (policy action = Ask). `PermissionRule.pattern` is matched
        // against `PermissionRequest::tool_name` by `MergedPolicy::evaluate`.
        let rule = PermissionRule {
            pattern: "bash".to_string(),
            action: PermissionAction::Ask,
            forced: false,
        };
        let checker = Arc::new(PermissionChecker::new(MergedPolicy::new(
            &[rule],
            &[],
            &[],
        )));
        let store = ApprovalStore::new();
        let service =
            TerminalApprovalService::new_with_checker(store, checker.clone());

        // Before record_always_decision: check returns RequireConfirm.
        let req = PermissionRequest::new(
            "bash".to_string(),
            serde_json::json!({"command": "cargo build"}),
            true,
        );
        let decisions =
            checker.check(std::slice::from_ref(&req)).await.unwrap();
        assert_eq!(decisions.get("bash"), Some(&Permission::RequireConfirm));

        // Record "always" decision via the extracted wiring method.
        service.record_always_decision(
            "bash",
            &serde_json::json!({"command": "cargo build"}),
        );

        // After record_always_decision: check returns AutoApprove,
        // proving the "always" decision propagated to the checker.
        let decisions =
            checker.check(std::slice::from_ref(&req)).await.unwrap();
        assert_eq!(decisions.get("bash"), Some(&Permission::AutoApprove));
    }

    #[tokio::test]
    async fn test_terminal_approval_always_wiring_is_key_order_independent() {
        use crate::{
            level::Permission,
            merged_policy::MergedPolicy,
            rule::{PermissionAction, PermissionRule},
            types::PermissionRequest,
        };

        // Same wiring as above, but verify that record_always_decision
        // normalizes args before forwarding to the checker: a request
        // whose JSON keys are in a different order must still match.
        let rule = PermissionRule {
            pattern: "bash".to_string(),
            action: PermissionAction::Ask,
            forced: false,
        };
        let checker = Arc::new(PermissionChecker::new(MergedPolicy::new(
            &[rule],
            &[],
            &[],
        )));
        let store = ApprovalStore::new();
        let service =
            TerminalApprovalService::new_with_checker(store, checker.clone());

        // Record "always" with keys in one order.
        service.record_always_decision(
            "bash",
            &serde_json::json!({"command": "ls", "cwd": "/tmp"}),
        );

        // A request with keys in the opposite order must still match.
        let req = PermissionRequest::new(
            "bash".to_string(),
            serde_json::json!({"cwd": "/tmp", "command": "ls"}),
            true,
        );
        let decisions =
            checker.check(std::slice::from_ref(&req)).await.unwrap();
        assert_eq!(decisions.get("bash"), Some(&Permission::AutoApprove));
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

    // ---- TerminalApprovalService interactive input handling ----

    fn terminal_service() -> (TerminalApprovalService, serde_json::Value) {
        (
            TerminalApprovalService::new(ApprovalStore::new()),
            serde_json::json!({"command": "ls"}),
        )
    }

    async fn terminal_with_input(
        service: &TerminalApprovalService,
        args: &serde_json::Value,
        input: &str,
    ) -> Result<ApprovalOutcome, ApprovalError> {
        terminal_with_input_and_policy(
            service,
            args,
            input,
            ApprovalPolicy::Once,
        )
        .await
    }

    async fn terminal_with_input_and_policy(
        service: &TerminalApprovalService,
        args: &serde_json::Value,
        input: &str,
        policy: ApprovalPolicy,
    ) -> Result<ApprovalOutcome, ApprovalError> {
        let reader = tokio::io::BufReader::new(input.as_bytes());
        let writer: Vec<u8> = Vec::new();
        service
            .request_approval_with_io(
                "bash",
                args,
                policy,
                Duration::from_secs(30),
                CancellationToken::new(),
                reader,
                writer,
            )
            .await
    }

    #[tokio::test]
    async fn terminal_approval_service_approves_once_on_y() {
        let (service, args) = terminal_service();
        let result = terminal_with_input(&service, &args, "y\n").await;
        assert_eq!(result, Ok(ApprovalOutcome::Approve));
    }

    #[tokio::test]
    async fn terminal_approval_service_approves_once_on_yes() {
        let (service, args) = terminal_service();
        let result = terminal_with_input(&service, &args, "yes\n").await;
        assert_eq!(result, Ok(ApprovalOutcome::Approve));
    }

    #[tokio::test]
    async fn terminal_approval_service_denies_on_n() {
        let (service, args) = terminal_service();
        let result = terminal_with_input(&service, &args, "n\n").await;
        assert_eq!(result, Ok(ApprovalOutcome::Deny));
    }

    #[tokio::test]
    async fn terminal_approval_service_denies_on_no() {
        let (service, args) = terminal_service();
        let result = terminal_with_input(&service, &args, "no\n").await;
        assert_eq!(result, Ok(ApprovalOutcome::Deny));
    }

    #[tokio::test]
    async fn terminal_approval_service_denies_on_reject() {
        let (service, args) = terminal_service();
        let result = terminal_with_input(&service, &args, "reject\n").await;
        assert_eq!(result, Ok(ApprovalOutcome::Deny));
    }

    #[tokio::test]
    async fn terminal_approval_service_approves_always_on_a() {
        let (service, args) = terminal_service();
        let result = terminal_with_input(&service, &args, "a\n").await;
        assert_eq!(result, Ok(ApprovalOutcome::Approve));
    }

    #[tokio::test]
    async fn terminal_approval_service_approves_always_on_always() {
        let (service, args) = terminal_service();
        let result = terminal_with_input(&service, &args, "always\n").await;
        assert_eq!(result, Ok(ApprovalOutcome::Approve));
    }

    #[tokio::test]
    async fn terminal_approval_service_approves_always_on_always_for_session() {
        let (service, args) = terminal_service();
        let result =
            terminal_with_input(&service, &args, "always-for-session\n").await;
        assert_eq!(result, Ok(ApprovalOutcome::Approve));
    }

    #[tokio::test]
    async fn terminal_approval_service_denies_on_unknown_input() {
        let (service, args) = terminal_service();
        let result = terminal_with_input(&service, &args, "maybe\n").await;
        assert_eq!(result, Ok(ApprovalOutcome::Deny));
    }

    #[tokio::test]
    async fn terminal_approval_service_caches_once_decision() {
        let (service, args) = terminal_service();
        assert_eq!(
            terminal_with_input(&service, &args, "y\n").await,
            Ok(ApprovalOutcome::Approve)
        );
        // Cache hit returns Approve immediately, ignoring the new input.
        assert_eq!(
            terminal_with_input(&service, &args, "n\n").await,
            Ok(ApprovalOutcome::Approve)
        );
        // Once decision was consumed; the next call prompts again.
        assert_eq!(
            terminal_with_input(&service, &args, "n\n").await,
            Ok(ApprovalOutcome::Deny)
        );
    }

    #[tokio::test]
    async fn terminal_approval_service_caches_always_for_session_decision() {
        let (service, args) = terminal_service();
        assert_eq!(
            terminal_with_input(&service, &args, "a\n").await,
            Ok(ApprovalOutcome::Approve)
        );
        // Always-for-session persists across lookups under its own policy.
        assert_eq!(
            terminal_with_input_and_policy(
                &service,
                &args,
                "n\n",
                ApprovalPolicy::AlwaysForSession
            )
            .await,
            Ok(ApprovalOutcome::Approve)
        );
    }

    #[tokio::test]
    async fn terminal_approval_service_caches_reject_decision() {
        let (service, args) = terminal_service();
        assert_eq!(
            terminal_with_input(&service, &args, "n\n").await,
            Ok(ApprovalOutcome::Deny)
        );
        // Reject persists and overrides subsequent input under its own policy.
        assert_eq!(
            terminal_with_input_and_policy(
                &service,
                &args,
                "y\n",
                ApprovalPolicy::Reject
            )
            .await,
            Ok(ApprovalOutcome::Deny)
        );
    }

    #[tokio::test]
    async fn terminal_approval_service_times_out() {
        let (service, args) = terminal_service();
        let reader = tokio::io::BufReader::new(tokio::io::empty());
        let writer: Vec<u8> = Vec::new();
        let result = service
            .request_approval_with_io(
                "bash",
                &args,
                ApprovalPolicy::Once,
                Duration::from_millis(10),
                CancellationToken::new(),
                reader,
                writer,
            )
            .await;
        assert_eq!(result, Err(ApprovalError::Timeout));
    }

    #[tokio::test]
    async fn terminal_approval_service_cancels() {
        let (service, args) = terminal_service();
        let reader = tokio::io::BufReader::new("y\n".as_bytes());
        let writer: Vec<u8> = Vec::new();
        let cancel = CancellationToken::new();
        cancel.cancel();
        let result = service
            .request_approval_with_io(
                "bash",
                &args,
                ApprovalPolicy::Once,
                Duration::from_secs(30),
                cancel,
                reader,
                writer,
            )
            .await;
        assert_eq!(result, Err(ApprovalError::Cancelled));
    }
}
