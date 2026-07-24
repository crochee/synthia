//! Subagent lifecycle: teams and lightweight per-task agents.
//!
//! [`SubagentManager`] owns the [`InMemoryMessageBus`] and
//! [`AgentCoordinator`] for a synthetic-agent deployment. It exposes
//! team CRUD plus a `create_agent` helper used by the `Agent` tool.

use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use dashmap::DashMap;
use parking_lot::Mutex as ParkingLotMutex;
use serde::Serialize;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::{
    bus::{InMemoryMessageBus, MessageBus},
    coordinator::{AgentCoordinator, AgentInstance},
};
use crate::config::AgentRunConfig;

#[derive(Debug, Clone, Serialize)]
pub struct Team {
    pub id: String,
    pub name: String,
    pub members: Vec<String>,
    pub created_at: String,
}

pub struct SubagentManager {
    message_bus: Arc<InMemoryMessageBus>,
    coordinator: Arc<AgentCoordinator>,
    teams: Arc<ParkingLotMutex<HashMap<String, Team>>>,
    // New fields for subagent execution
    max_depth: AtomicUsize,
    max_concurrent: AtomicUsize,
    active_count: AtomicUsize,
    parent_config: ParkingLotMutex<Option<AgentRunConfig>>,
    // Spawn depth of the agent that owns this manager. Root agent has
    // depth 0, direct children have depth 1, etc. Used by
    // `current_depth()` to enforce the `max_depth` limit in
    // `AgentTool::call`.
    //
    // Spec adaptation: the OpenSpec spec assumed a `SubagentConfig`
    // struct would carry `depth`, but that struct does not exist in the
    // codebase. Depth is tracked here on `SubagentManager` via an
    // `AtomicUsize` instead, preserving the spec's intent (depth
    // tracking + max_depth enforcement + child depth = parent + 1).
    depth: AtomicUsize,
    // Parent→children session mapping for recursive subtree cancellation
    // (spec: `subagent-tree-cancellation`). Each entry maps a parent
    // session id to the list of direct child session ids it has spawned.
    // Tracked separately from `coordinator` because cancellation needs
    // a session-id tree, while `coordinator` tracks agent instances by
    // agent id (a different identifier space).
    child_sessions: DashMap<String, Vec<String>>,
    // Per-session cancellation tokens. The root session's token is the
    // `AgentRunConfig::cancel_token` passed in via `set_parent_config`;
    // each child entry here is derived from its parent's token via
    // `CancellationToken::child_token()`. Canceling a parent's token
    // propagates to all descendants (existing behavior); canceling a
    // specific child's token via `cancel_session_tree` cancels only
    // that subtree.
    //
    // `DashMap` (not `Arc<DashMap>`) is sufficient because
    // `SubagentManager` itself is shared via `Arc<SubagentManager>`,
    // and `DashMap` exposes all mutating methods through `&self`
    // (internal sharding). The inner `Arc` would be redundant.
    session_cancel_tokens: DashMap<String, CancellationToken>,
}

impl SubagentManager {
    pub fn new() -> Self {
        let message_bus = Arc::new(InMemoryMessageBus::new());
        let coordinator = Arc::new(AgentCoordinator::new(message_bus.clone()));
        Self {
            message_bus,
            coordinator,
            teams: Arc::new(ParkingLotMutex::new(HashMap::new())),
            max_depth: AtomicUsize::new(3),
            max_concurrent: AtomicUsize::new(5),
            active_count: AtomicUsize::new(0),
            parent_config: ParkingLotMutex::new(None),
            depth: AtomicUsize::new(0),
            child_sessions: DashMap::new(),
            session_cancel_tokens: DashMap::new(),
        }
    }

    pub fn get_message_bus(&self) -> Arc<InMemoryMessageBus> {
        self.message_bus.clone()
    }

    pub fn get_coordinator(&self) -> Arc<AgentCoordinator> {
        self.coordinator.clone()
    }

    pub fn create_team(&self, name: &str, members: Vec<String>) -> Team {
        let team = Team {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            members,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        self.teams.lock().insert(team.id.clone(), team.clone());
        team
    }

    pub fn delete_team(&self, id: &str) -> bool {
        self.teams.lock().remove(id).is_some()
    }

    pub fn create_agent(&self, task: &str) -> String {
        let id = Uuid::new_v4().to_string();
        let agent = AgentInstance::new(
            id.clone(),
            "worker".to_string(),
            vec![],
            task.to_string(),
            vec![],
            HashMap::new(),
        );
        // Register with message bus
        let _ = self.message_bus.register_agent(&id);
        // Register with coordinator
        let _ = self.coordinator.register_agent(agent);
        id
    }

    pub fn send_message(&self, agent_id: &str, _message: &str) -> bool {
        self.coordinator.get_agent(agent_id).is_ok()
    }

    // ── New methods for subagent execution ──

    /// Current spawn depth of the agent that owns this manager.
    ///
    /// Root agent has depth 0, direct children have depth 1, etc.
    /// Returns the actual depth stored via [`set_depth`] instead of a
    /// stub value, so `AgentTool::call` can enforce the `max_depth`
    /// limit correctly.
    pub fn current_depth(&self) -> usize {
        self.depth.load(Ordering::Relaxed)
    }

    /// Set the spawn depth of the agent that owns this manager.
    ///
    /// Called when a child session is created: the child's depth is
    /// `parent_depth + 1`. The root agent leaves this at the default
    /// value of 0.
    pub fn set_depth(&self, depth: usize) {
        self.depth.store(depth, Ordering::Relaxed);
    }

    /// Maximum allowed sub-agent nesting depth.
    pub fn max_depth(&self) -> usize {
        self.max_depth.load(Ordering::Relaxed)
    }

    /// Maximum allowed concurrent sub-agents.
    pub fn max_concurrent(&self) -> usize {
        self.max_concurrent.load(Ordering::Relaxed)
    }

    /// Try to acquire a concurrency slot.
    ///
    /// On success, returns a [`SlotGuard`] that owns the slot. The slot
    /// is released automatically when the guard is dropped, unless
    /// [`SlotGuard::commit`] is called first (which keeps the slot held
    /// so the caller can manage it explicitly across `.await` points).
    ///
    /// Returns `None` when `active_count >= max_concurrent` (quota
    /// exhausted); no slot is consumed in that case.
    pub fn try_acquire_slot(self: &Arc<Self>) -> Option<SlotGuard> {
        let max = self.max_concurrent.load(Ordering::Relaxed);
        loop {
            let current = self.active_count.load(Ordering::Relaxed);
            if current >= max {
                return None;
            }
            if self
                .active_count
                .compare_exchange(
                    current,
                    current + 1,
                    Ordering::Acquire,
                    Ordering::Relaxed,
                )
                .is_ok()
            {
                return Some(SlotGuard {
                    manager: Arc::clone(self),
                    released: false,
                });
            }
        }
    }

    /// Release a previously acquired concurrency slot.
    ///
    /// This is called automatically by [`SlotGuard::drop`] (when the
    /// guard has not been committed). Callers that used
    /// [`SlotGuard::commit`] must call this explicitly once the guarded
    /// operation completes.
    pub fn release_slot(&self) {
        self.active_count.fetch_sub(1, Ordering::Release);
    }

    /// Get the parent agent run configuration, if set.
    pub fn parent_config(&self) -> Option<AgentRunConfig> {
        self.parent_config.lock().clone()
    }

    /// Set the parent agent run configuration. Used when the main
    /// agent loop wires the subagent manager into the execution
    /// context.
    pub fn set_parent_config(&self, config: AgentRunConfig) {
        *self.parent_config.lock() = Some(config);
    }

    // ── Recursive subtree cancellation (spec: subagent-tree-cancellation) ──

    /// Register a parent→child session relationship and the child's
    /// dedicated cancellation token.
    ///
    /// Called by `AgentTool::call` before spawning the child via
    /// `run_child`. The `child_cancel_token` MUST be derived from the
    /// parent's `cancel_token` via `CancellationToken::child_token()` so
    /// that canceling the parent's shared token still propagates to all
    /// children (existing behavior), while `cancel_session_tree` can
    /// cancel a specific subtree in isolation.
    ///
    /// Idempotent: registering the same `(parent_id, child_id)` pair
    /// twice is a no-op — the child is not appended a second time.
    /// `session_cancel_tokens` is always overwritten with the latest
    /// token, which is correct because a re-registration implies the
    /// prior token is stale.
    pub fn register_child_session(
        &self,
        parent_id: String,
        child_id: String,
        child_cancel_token: CancellationToken,
    ) {
        self.child_sessions
            .entry(parent_id)
            .and_modify(|children| {
                if !children.contains(&child_id) {
                    children.push(child_id.clone());
                }
            })
            .or_insert_with(|| vec![child_id.clone()]);
        self.session_cancel_tokens
            .insert(child_id, child_cancel_token);
    }

    /// Remove a session from the tracking maps, recursively cleaning up
    /// all descendants first.
    ///
    /// Recursively cancels and removes each descendant's entries before
    /// removing `session_id` itself. This handles the background nesting
    /// case where a grandchild session may still be running when the
    /// child completes: the grandchild's token is canceled (so it
    /// stops), and its tracking entries are dropped. Without this
    /// recursion, a still-running grandchild would become an orphan —
    /// its `session_cancel_tokens` entry could never be reclaimed by a
    /// later `cancel_session_tree(root)` because no `child_sessions`
    /// entry would point to it.
    ///
    /// Removes the session id from its parent's child list (if any) and
    /// drops the session's own child-list entry and cancel token. Safe
    /// to call for a session that was never registered (no-op).
    ///
    /// Called by `AgentTool::call` after `run_child` returns (foreground
    /// path) or after the spawned background task completes (background
    /// path). This keeps `child_sessions` from growing unboundedly as
    /// new subagents are spawned.
    ///
    /// # Cancellation / Panic Safety
    ///
    /// Like `SlotGuard`, this method is called after `run_child` returns.
    /// If `run_child`'s future is dropped (parent cancellation) or panics,
    /// this method is NOT called, and the session's entries in
    /// `child_sessions` / `session_cancel_tokens` will leak. This is an
    /// accepted trade-off (same as `SlotGuard`'s slot leak on
    /// panic/cancel): the leaked entries are no-ops for future
    /// cancellations (the token is already orphaned) and the memory
    /// overhead is bounded by `max_concurrent`. A future hardening pass
    /// could wrap registration in an RAII guard with a `Drop` impl that
    /// calls `remove_session`.
    pub fn remove_session(&self, session_id: &str) {
        // Recursively cancel and clean up descendants first. This
        // handles the background nesting case where a grandchild may
        // still be running when the child completes — we cancel the
        // grandchild's token so it stops, then clean up its tracking
        // entries.
        let children: Vec<String> = self
            .child_sessions
            .get(session_id)
            .map(|entry| entry.value().clone())
            .unwrap_or_default();
        for child_id in children {
            self.remove_session(&child_id);
        }

        // Remove this session from its parent's child list (scan all
        // entries because we don't track parent pointers). DashMap's
        // `iter_mut()` yields write guards one entry at a time, so we
        // never hold more than one shard lock at once — no deadlock
        // risk.
        //
        // Collect keys whose child list becomes empty after removal so
        // we can drop them after the iteration (DashMap does not allow
        // removal during `iter_mut`).
        let mut empty_keys: Vec<String> = Vec::new();
        for mut entry in self.child_sessions.iter_mut() {
            let before = entry.value().len();
            entry.value_mut().retain(|id| id != session_id);
            if before > 0 && entry.value().is_empty() {
                empty_keys.push(entry.key().clone());
            }
        }
        for key in empty_keys {
            self.child_sessions.remove(&key);
        }
        // Remove this session's own child-list entry (it may have been
        // a parent). By this point all descendants have been removed,
        // so this entry is either already gone or empty.
        self.child_sessions.remove(session_id);
        // Cancel and drop its cancel token entry. Canceling is a no-op
        // if the session already completed; it ensures any
        // still-running descendant (which was canceled in the recursive
        // call above) is fully signaled.
        if let Some((_, token)) = self.session_cancel_tokens.remove(session_id)
        {
            token.cancel();
        }
    }

    /// Recursively cancel a session and all its descendants.
    ///
    /// Performs a depth-first traversal of `child_sessions`: cancels
    /// each descendant's token before canceling the target session's
    /// own token. This ordering ensures that by the time the target's
    /// token is canceled, all descendants have already been signaled,
    /// matching the spec scenario "Cancel parent cancels all
    /// descendants" (parent itself is canceled last).
    ///
    /// # Concurrency
    ///
    /// Children are collected into a `Vec<String>` before recursion so
    /// the DashMap read guard is released immediately. If a child is
    /// concurrently removed between collection and recursion, the
    /// recursive call simply finds no token entry for it and skips it
    /// (no panic). Remaining children are still canceled.
    ///
    /// **Note**: Children registered to `session_id` AFTER the snapshot
    /// is taken will NOT be canceled by this call. Callers that need to
    /// guarantee cancellation of all descendants (including those
    /// concurrently registered) should cancel the session's
    /// `AgentRunConfig::cancel_token` instead, which propagates via
    /// tokio_util's token hierarchy.
    ///
    /// # Deadlock safety
    ///
    /// No DashMap guard is held across the recursive call.
    pub fn cancel_session_tree(&self, session_id: &str) {
        // Snapshot the child list, then release the guard.
        let children: Vec<String> = self
            .child_sessions
            .get(session_id)
            .map(|entry| entry.value().clone())
            .unwrap_or_default();

        // Recurse into each child subtree first.
        for child_id in children {
            self.cancel_session_tree(&child_id);
        }

        // Cancel this session's token last.
        if let Some(entry) = self.session_cancel_tokens.get(session_id) {
            entry.cancel();
        }
    }
}

#[cfg(test)]
impl SubagentManager {
    /// Test-only: check whether `session_id` has an entry in
    /// `child_sessions` (i.e. it has been registered as a parent).
    pub(crate) fn has_child_session_entry(&self, session_id: &str) -> bool {
        self.child_sessions.contains_key(session_id)
    }

    /// Test-only: check whether `session_id` has an entry in
    /// `session_cancel_tokens`.
    pub(crate) fn has_cancel_token_entry(&self, session_id: &str) -> bool {
        self.session_cancel_tokens.contains_key(session_id)
    }

    /// Test-only: snapshot of the child list recorded for `session_id`.
    pub(crate) fn children_of(&self, session_id: &str) -> Vec<String> {
        self.child_sessions
            .get(session_id)
            .map(|entry| entry.value().clone())
            .unwrap_or_default()
    }
}

/// RAII guard for a subagent concurrency slot.
///
/// Created by [`SubagentManager::try_acquire_slot`]. The slot is
/// released automatically when the guard is dropped, **unless**
/// [`SlotGuard::commit`] was called first.
///
/// ## Usage across `.await` points
///
/// `SlotGuard` MUST NOT be held across `.await` points. For foreground
/// subagent execution, call [`SlotGuard::commit`] before the await;
/// this consumes the guard (so Drop will not release the slot) and the
/// caller becomes responsible for calling
/// [`SubagentManager::release_slot`] once the awaited operation
/// completes. For background execution, simply drop the guard after
/// spawning — the slot is released immediately, matching the
/// "background tasks run independently" semantics.
///
/// ## Rationale
///
/// The "no-across-await" rule is a deliberate design choice, not a
/// memory-safety requirement:
///
/// - **Explicit release timing.** Holding the guard across a potentially
///   long-running subagent execution would tie slot lifetime to the
///   opaque future, making it hard to reason about when slots return to
///   the pool. Forcing `commit()` before the await makes the release
///   point a visible, explicit call site in the caller.
/// - **Avoids holding the manager `Arc` across subagent execution.**
///   `SlotGuard` owns an `Arc<SubagentManager>`; carrying it across a
///   long await (which may span many executor polls) keeps that refcount
///   live unnecessarily and obscures ownership flow.
///
/// ## Trade-off: slot leak on panic / cancellation
///
/// Because `commit()` is called *before* `run_child().await`, if that
/// await panics or the future is dropped (cancellation) before reaching
/// the manual `release_slot()` call, the slot is leaked: the guard is
/// already consumed so `Drop` will not release it, and the cleanup line
/// never runs. This is an **accepted trade-off**:
///
/// - Subagent panics are rare in practice (subagents trap their own
///   errors and return `AgentStatus::Errored`).
/// - A leaked slot only reduces concurrency capacity for the remainder
///   of the manager's lifetime; the manager can be reset/recreated to
///   recover full capacity.
/// - The alternative (holding the guard across the await and relying on
///   `Drop` for cleanup) sacrifices the explicit-release-timing
///   property above, which we value more than rare-leak recovery.
pub struct SlotGuard {
    manager: Arc<SubagentManager>,
    released: bool,
}

impl SlotGuard {
    /// Commit the guard: the slot stays consumed and [`Drop`] will NOT
    /// call [`SubagentManager::release_slot`].
    ///
    /// Use this before awaiting a foreground subagent task. After the
    /// await completes, the caller must explicitly call
    /// [`SubagentManager::release_slot`] to balance the acquire.
    pub fn commit(mut self) {
        self.released = true;
    }
}

impl Drop for SlotGuard {
    fn drop(&mut self) {
        if !self.released {
            self.manager.release_slot();
        }
    }
}

impl Default for SubagentManager {
    fn default() -> Self {
        Self::new()
    }
}
