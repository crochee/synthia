//! Agent-loop extension points: 12 typed hook points fired by the agent
//! runtime at well-defined lifecycle events.
//!
//! # Design
//!
//! - **Typed payloads** (per P9 observability): every hook carries a
//!   strongly-typed struct, not a `serde_json::Value`. The
//!   [`AgentLoopEvent::payload()`] method returns the typed value.
//! - **No data flow back from handlers** (yet): handlers are `Fn` not
//!   `FnMut`; we do not currently support hooks that mutate the agent
//!   state. This will be added in Phase 4 (Scope 4: Context extension
//!   points include `messages.transform` etc.) via a separate `Action<...>`
//!   return type.
//! - **Idempotent registration**: re-registering the same handler is a
//!   no-op (DashMap-based dedup). This is a deliberate choice to make
//!   hot-swap safe.
//! - **OTel-friendly**: every fire emits a `tracing::span!` with
//!   `point.name` and `extension.count` (P9 requirement). The span is
//!   a no-op without the `otel` feature.
//!
//! # Points
//!
//! | Name | Payload | Fired at |
//! |------|---------|----------|
//! | `agent_start` | `AgentStart` | before the first iteration |
//! | `agent_end` | `AgentEnd` | after the final iteration |
//! | `turn_start` | `TurnStart` | before each user turn |
//! | `turn_end` | `TurnEnd` | after each assistant turn |
//! | `iteration_start` | `IterationStart` | before each iteration |
//! | `iteration_end` | `IterationEnd` | after each iteration |
//! | `error` | `Error` | on any runtime error |
//! | `compact_start` | `CompactStart` | before a compaction |
//! | `compact_end` | `CompactEnd` | after a compaction |
//! | `branch_navigate` | `BranchNavigate` | on a session branch switch |
//! | `session_start` | `SessionStart` | when a session is created |
//! | `session_end` | `SessionEnd` | when a session is closed |

use std::sync::Arc;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};

/// Typed payloads for each agent-loop extension point.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentLoopEvent {
    AgentStart(AgentStart),
    AgentEnd(AgentEnd),
    TurnStart(TurnStart),
    TurnEnd(TurnEnd),
    IterationStart(IterationStart),
    IterationEnd(IterationEnd),
    Error(ErrorEvent),
    CompactStart(CompactStart),
    CompactEnd(CompactEnd),
    BranchNavigate(BranchNavigate),
    SessionStart(SessionStart),
    SessionEnd(SessionEnd),
}

impl AgentLoopEvent {
    /// Stable string name for the point (used as DashMap key + OTel attribute).
    pub fn point_name(&self) -> &'static str {
        match self {
            Self::AgentStart(_) => "agent_start",
            Self::AgentEnd(_) => "agent_end",
            Self::TurnStart(_) => "turn_start",
            Self::TurnEnd(_) => "turn_end",
            Self::IterationStart(_) => "iteration_start",
            Self::IterationEnd(_) => "iteration_end",
            Self::Error(_) => "error",
            Self::CompactStart(_) => "compact_start",
            Self::CompactEnd(_) => "compact_end",
            Self::BranchNavigate(_) => "branch_navigate",
            Self::SessionStart(_) => "session_start",
            Self::SessionEnd(_) => "session_end",
        }
    }

    /// Serialized payload size estimate (rough — for OTel `event.size`
    /// attribute).
    pub fn payload_size(&self) -> usize {
        serde_json::to_vec(self).map(|v| v.len()).unwrap_or(0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStart {
    pub session_id: String,
    pub user_id: String,
    pub agent_id: String,
    pub input_summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentEnd {
    pub session_id: String,
    pub iterations: u32,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnStart {
    pub session_id: String,
    pub turn_id: u32,
    pub user_input: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnEnd {
    pub session_id: String,
    pub turn_id: u32,
    pub assistant_output: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IterationStart {
    pub session_id: String,
    pub iteration: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IterationEnd {
    pub session_id: String,
    pub iteration: u32,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ErrorSeverity {
    Recoverable,
    Warning,
    Fatal,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ErrorSource {
    Llm,
    Tool,
    Permission,
    Internal,
    Provider,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorEvent {
    pub session_id: String,
    pub severity: ErrorSeverity,
    pub source: ErrorSource,
    pub recoverable: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactStart {
    pub session_id: String,
    pub trigger: String,
    pub messages_before: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactEnd {
    pub session_id: String,
    pub messages_before: usize,
    pub messages_after: usize,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchNavigate {
    pub session_id: String,
    pub from_id: String,
    pub to_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStart {
    pub session_id: String,
    pub user_id: String,
    pub workspace_root: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEnd {
    pub session_id: String,
    pub duration_ms: u64,
    pub final_state: String,
}

/// Handler signature for agent-loop extension points.
pub type AgentLoopHandler = Arc<dyn Fn(&AgentLoopEvent) + Send + Sync>;

/// A registry of agent-loop extension handlers. Handlers are stored in a
/// `DashMap<&'static str, Vec<HandlerEntry>>` keyed by point name.
pub struct AgentLoopExtensionRegistry {
    handlers: DashMap<&'static str, Vec<HandlerEntry>>,
}

struct HandlerEntry {
    id: String,
    handler: AgentLoopHandler,
}

impl std::fmt::Debug for AgentLoopExtensionRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentLoopExtensionRegistry")
            .field("handler_count", &self.total_handler_count())
            .finish()
    }
}

impl Default for AgentLoopExtensionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentLoopExtensionRegistry {
    /// Build an empty registry.
    pub fn new() -> Self {
        Self {
            handlers: DashMap::new(),
        }
    }

    /// Register a handler for the given point. Re-registering the same
    /// `handler_id` for the same point is a no-op (idempotent).
    pub fn register(
        &self,
        point_name: &'static str,
        handler_id: impl Into<String>,
        handler: AgentLoopHandler,
    ) {
        let handler_id = handler_id.into();
        let mut entry = self.handlers.entry(point_name).or_default();
        if entry.iter().any(|e| e.id == handler_id) {
            return;
        }
        entry.push(HandlerEntry {
            id: handler_id,
            handler,
        });
    }

    /// Unregister a handler by id from the given point. Returns `true` if
    /// a handler was removed. If the point ends up with no handlers, the
    /// entry is removed from the map so `active_points()` reflects the
    /// change.
    pub fn unregister(
        &self,
        point_name: &'static str,
        handler_id: &str,
    ) -> bool {
        let mut removed = false;
        if let Some(mut entry) = self.handlers.get_mut(point_name) {
            let before = entry.len();
            entry.retain(|e| e.id != handler_id);
            removed = entry.len() < before;
            if entry.is_empty() {
                drop(entry);
                self.handlers.remove(point_name);
            }
        }
        removed
    }

    /// Number of handlers registered for a point.
    pub fn handler_count(&self, point_name: &str) -> usize {
        self.handlers
            .get(point_name)
            .map(|e| e.value().len())
            .unwrap_or(0)
    }

    /// Total number of handlers across all points.
    pub fn total_handler_count(&self) -> usize {
        self.handlers.iter().map(|e| e.value().len()).sum()
    }

    /// List of all point names with at least one handler.
    pub fn active_points(&self) -> Vec<&'static str> {
        self.handlers.iter().map(|e| *e.key()).collect()
    }

    /// Fire an event. Every registered handler for the point is invoked
    /// synchronously (in registration order). A handler panic is caught
    /// and logged so one bad handler cannot take down the agent loop.
    ///
    /// Emits a `tracing::info_span!` named `extension.hook.<point>` with
    /// `extension_id` (per-handler) and `scope = "agent_loop"` so that
    /// OTel consumers can attribute the hook fire to a specific
    /// extension (P9 observability requirement).
    pub fn fire(&self, event: &AgentLoopEvent) {
        let point_name = event.point_name();
        let payload_size = event.payload_size();
        let scope = "agent_loop";
        if let Some(entry) = self.handlers.get(point_name) {
            for handler in entry.value() {
                let handler_id = handler.id.clone();
                let _span = tracing::info_span!(
                    target: "synthia.extension",
                    "extension.hook",
                    point = point_name,
                    scope = scope,
                    extension_id = handler_id.as_str(),
                    payload_size = payload_size,
                )
                .entered();
                // Synchronous dispatch — agent-loop extension points are
                // observational/advisory and must complete before the next
                // iteration. Phase 4 may add async dispatch for specific
                // points (e.g. `messages.transform`).
                let result = std::panic::catch_unwind(
                    std::panic::AssertUnwindSafe(|| (handler.handler)(event)),
                );
                if result.is_err() {
                    eprintln!(
                        "agent-loop extension handler `{}` for point `{}` panicked",
                        handler_id, point_name
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    fn make_handler() -> (AgentLoopHandler, Arc<Mutex<Vec<AgentLoopEvent>>>) {
        let captured: Arc<Mutex<Vec<AgentLoopEvent>>> =
            Arc::new(Mutex::new(Vec::new()));
        let cap = captured.clone();
        let handler: AgentLoopHandler = Arc::new(move |ev: &AgentLoopEvent| {
            cap.lock().unwrap().push(ev.clone());
        });
        (handler, captured)
    }

    fn sample_event() -> AgentLoopEvent {
        AgentLoopEvent::AgentStart(AgentStart {
            session_id: "s1".to_string(),
            user_id: "u1".to_string(),
            agent_id: "a1".to_string(),
            input_summary: "hello".to_string(),
        })
    }

    #[test]
    fn new_registry_is_empty() {
        let reg = AgentLoopExtensionRegistry::new();
        assert_eq!(reg.total_handler_count(), 0);
        assert!(reg.active_points().is_empty());
    }

    #[test]
    fn register_and_fire_delivers_event() {
        let reg = AgentLoopExtensionRegistry::new();
        let (h, captured) = make_handler();
        reg.register("agent_start", "h1", h);
        assert_eq!(reg.handler_count("agent_start"), 1);

        reg.fire(&sample_event());
        let evts = captured.lock().unwrap();
        assert_eq!(evts.len(), 1);
        assert_eq!(evts[0].point_name(), "agent_start");
    }

    #[test]
    fn register_is_idempotent_for_same_id() {
        let reg = AgentLoopExtensionRegistry::new();
        let (h1, _) = make_handler();
        let (h2, _) = make_handler();
        reg.register("agent_start", "h1", h1);
        reg.register("agent_start", "h1", h2);
        assert_eq!(reg.handler_count("agent_start"), 1);
    }

    #[test]
    fn register_multiple_distinct_ids() {
        let reg = AgentLoopExtensionRegistry::new();
        let (h1, _) = make_handler();
        let (h2, _) = make_handler();
        reg.register("agent_start", "h1", h1);
        reg.register("agent_start", "h2", h2);
        assert_eq!(reg.handler_count("agent_start"), 2);
    }

    #[test]
    fn unregister_removes_handler() {
        let reg = AgentLoopExtensionRegistry::new();
        let (h1, _) = make_handler();
        let (h2, _) = make_handler();
        reg.register("agent_start", "h1", h1);
        reg.register("agent_start", "h2", h2);
        assert!(reg.unregister("agent_start", "h1"));
        assert!(!reg.unregister("agent_start", "h1")); // second call is no-op
        assert!(!reg.unregister("nonexistent", "x"));
        assert_eq!(reg.handler_count("agent_start"), 1);
    }

    #[test]
    fn fire_with_no_handlers_is_noop() {
        let reg = AgentLoopExtensionRegistry::new();
        reg.fire(&sample_event());
    }

    #[test]
    fn handler_panic_is_caught() {
        let reg = AgentLoopExtensionRegistry::new();
        let bad: AgentLoopHandler = Arc::new(|_| {
            panic!("boom");
        });
        reg.register("agent_start", "bad", bad);
        // Should not panic the test.
        reg.fire(&sample_event());
    }

    #[test]
    fn fire_delivers_to_correct_point_only() {
        let reg = AgentLoopExtensionRegistry::new();
        let (h, captured) = make_handler();
        reg.register("turn_start", "h1", h);
        reg.fire(&AgentLoopEvent::AgentStart(AgentStart {
            session_id: "s1".to_string(),
            user_id: "u1".to_string(),
            agent_id: "a1".to_string(),
            input_summary: "x".to_string(),
        }));
        assert!(captured.lock().unwrap().is_empty());
    }

    #[test]
    fn point_names_are_stable() {
        let samples = [
            (
                AgentLoopEvent::AgentStart(AgentStart {
                    session_id: "s".to_string(),
                    user_id: "u".to_string(),
                    agent_id: "a".to_string(),
                    input_summary: "x".to_string(),
                }),
                "agent_start",
            ),
            (
                AgentLoopEvent::Error(ErrorEvent {
                    session_id: "s".to_string(),
                    severity: ErrorSeverity::Warning,
                    source: ErrorSource::Internal,
                    recoverable: true,
                    message: "x".to_string(),
                }),
                "error",
            ),
            (
                AgentLoopEvent::CompactEnd(CompactEnd {
                    session_id: "s".to_string(),
                    messages_before: 100,
                    messages_after: 50,
                    duration_ms: 12,
                }),
                "compact_end",
            ),
        ];
        for (ev, name) in samples {
            assert_eq!(ev.point_name(), name);
        }
    }

    #[test]
    fn payload_serializes() {
        let ev = sample_event();
        let json = serde_json::to_string(&ev).unwrap();
        let back: AgentLoopEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(back.point_name(), "agent_start");
    }

    #[test]
    fn active_points_lists_only_nonempty() {
        let reg = AgentLoopExtensionRegistry::new();
        let (h, _) = make_handler();
        reg.register("agent_start", "h1", h.clone());
        reg.register("turn_end", "h1", h);
        reg.unregister("turn_end", "h1");
        let active = reg.active_points();
        assert_eq!(active, vec!["agent_start"]);
    }

    // --- Phase 3.4: concurrent dispatch tests (DashMap is internally synchronized) ---

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_register_does_not_lose_handlers() {
        let reg = std::sync::Arc::new(AgentLoopExtensionRegistry::new());
        let counter =
            std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut handles = Vec::new();
        for i in 0..64 {
            let reg = reg.clone();
            let counter = counter.clone();
            handles.push(tokio::spawn(async move {
                let id = format!("h{}", i);
                let counter = counter.clone();
                let h: AgentLoopHandler = std::sync::Arc::new(move |_ev| {
                    counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                });
                reg.register("agent_start", id, h);
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
        assert_eq!(reg.handler_count("agent_start"), 64);

        reg.fire(&sample_event());
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 64);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_fire_is_thread_safe() {
        // DashMap is internally synchronized — many fire() calls in
        // parallel must not deadlock or panic.
        let reg = std::sync::Arc::new(AgentLoopExtensionRegistry::new());
        let counter =
            std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter_clone = counter.clone();
        let h: AgentLoopHandler = std::sync::Arc::new(move |_ev| {
            counter_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        });
        reg.register("agent_start", "shared", h);

        let mut handles = Vec::new();
        for _ in 0..32 {
            let reg = reg.clone();
            handles.push(tokio::spawn(async move {
                for _ in 0..16 {
                    reg.fire(&sample_event());
                }
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
        // 32 tasks * 16 fires = 512 deliveries to the single shared handler.
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 512);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_register_and_fire_does_not_deadlock() {
        // Mixed workload: some tasks register, some fire. The DashMap
        // must support this without deadlocking.
        let reg = std::sync::Arc::new(AgentLoopExtensionRegistry::new());
        let counter =
            std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));

        let mut handles = Vec::new();
        // 8 registering tasks
        for i in 0..8 {
            let reg = reg.clone();
            let counter = counter.clone();
            handles.push(tokio::spawn(async move {
                for j in 0..8 {
                    let id = format!("r{}-h{}", i, j);
                    let counter = counter.clone();
                    let h: AgentLoopHandler = std::sync::Arc::new(move |_ev| {
                        counter
                            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    });
                    reg.register("agent_start", id, h);
                }
            }));
        }
        // 8 firing tasks
        for _ in 0..8 {
            let reg = reg.clone();
            handles.push(tokio::spawn(async move {
                for _ in 0..16 {
                    reg.fire(&sample_event());
                }
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
        // 8 register tasks * 8 handlers each = 64 registered.
        assert_eq!(reg.handler_count("agent_start"), 64);
        // Each fire could have been intercepted by 0..64 handlers; we
        // just assert that no panics/deadlocks occurred and the counter
        // is non-zero (at least one handler fired).
        let observed = counter.load(std::sync::atomic::Ordering::SeqCst);
        assert!(observed > 0, "expected some handler fires, got 0");
    }
}
