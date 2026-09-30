//! [`SessionLane`] — the lane abstraction over a session
//! controller (R4 Phase E, pi `Lane` semantics).
//!
//! A *lane* is one independently driven conversation line: it
//! accepts operations, publishes agent events, and closes
//! cleanly. [`SessionController`] already provides exactly this
//! surface for the single-conversation topology Synthia ships
//! today; the trait exists so future multi-lane fan-out (pi's
//! `AgentHarness` lanes, parallel session forking) can be built
//! against a narrow interface instead of the concrete controller.
//!
//! [`DefaultLane`] is the only implementation: a transparent
//! delegation over [`SessionController`] with zero behavioral
//! change.

use std::sync::Arc;

use synthia::harness::AgentEvent;
use tokio::sync::broadcast;

use super::controller::{SessionController, SessionOp};

/// Errors a lane can surface.
#[derive(Debug)]
pub enum LaneError {
    /// The underlying controller rejected the submit (closed,
    /// poisoned channel, or shutdown race).
    Submit(String),
    /// The lane is closed and accepts no further operations.
    Closed,
}

impl std::fmt::Display for LaneError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Submit(e) => write!(f, "lane submit failed: {e}"),
            Self::Closed => write!(f, "lane is closed"),
        }
    }
}

impl std::error::Error for LaneError {}

/// One independently driven conversation line.
///
/// The interface is deliberately three methods: submit,
/// subscribe, close. Anything richer is topology policy and
/// stays on the concrete implementation.
pub trait SessionLane: Send + Sync {
    /// Stable lane identifier (the session id for the default
    /// single-lane topology).
    fn lane_id(&self) -> &str;

    /// Submit one operation to the lane's serialized command
    /// loop.
    fn submit(
        &self,
        op: SessionOp,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), LaneError>> + Send>,
    >;

    /// Subscribe to the lane's broadcast agent-event stream.
    fn subscribe(&self) -> broadcast::Receiver<AgentEvent>;

    /// Close the lane. Idempotent.
    fn close(
        &self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), LaneError>> + Send>,
    >;
}

/// Transparent delegation over [`SessionController`].
///
/// Every method forwards 1:1; constructing one changes no
/// behavior. It exists so callers can hold `Arc<dyn SessionLane>`
/// (or `Vec<Arc<dyn SessionLane>>` for future fan-out) instead
/// of the concrete type.
pub struct DefaultLane {
    controller: Arc<SessionController>,
}

impl DefaultLane {
    /// Wrap a controller.
    #[must_use]
    pub fn new(controller: Arc<SessionController>) -> Self {
        Self { controller }
    }

    /// The wrapped controller (for callers that need the
    /// concrete type, e.g. state queries).
    #[must_use]
    pub fn controller(&self) -> &Arc<SessionController> {
        &self.controller
    }
}

impl SessionLane for DefaultLane {
    fn lane_id(&self) -> &str {
        self.controller.session_id()
    }

    fn close(
        &self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), LaneError>> + Send>,
    > {
        let controller = Arc::clone(&self.controller);
        Box::pin(async move {
            controller
                .submit(SessionOp::Shutdown)
                .await
                .map_err(|e| LaneError::Submit(e.to_string()))
        })
    }

    fn submit(
        &self,
        op: SessionOp,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), LaneError>> + Send>,
    > {
        let controller = Arc::clone(&self.controller);
        Box::pin(async move {
            controller
                .submit(op)
                .await
                .map_err(|e| LaneError::Submit(e.to_string()))
        })
    }

    fn subscribe(&self) -> broadcast::Receiver<AgentEvent> {
        self.controller.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, time::Duration};

    use synthia::{test_support::FakeProvider, tool::registry::ToolRegistry};
    use tokio::sync::RwLock;

    use super::*;
    use crate::session::controller::{AgentRunStreamFactory, RunDependencies};

    async fn lane(idle: Duration) -> DefaultLane {
        let temp = tempfile::TempDir::new().unwrap();
        let manager = synthia::session::manager::SessionRegistry::new(
            temp.path().to_path_buf(),
        );
        manager
            .create_with_user("lane-1".to_string(), "alice".to_string())
            .await
            .unwrap();
        let deps = RunDependencies::new(
            Arc::new(FakeProvider::new(vec![])),
            Arc::new(RwLock::new(ToolRegistry::new())),
            PathBuf::from("/tmp"),
            synthia::harness::DEFAULT_SYSTEM_PROMPT.to_string(),
        );
        let controller = SessionController::spawn(
            "alice",
            "lane-1",
            manager.input_queue(),
            manager.sink("alice", "lane-1"),
            deps,
            idle,
            Arc::new(AgentRunStreamFactory),
        );
        DefaultLane::new(controller)
    }

    /// The lane id MUST echo the controller's session id.
    #[tokio::test]
    async fn lane_id_matches_session_id() {
        let lane = lane(Duration::from_secs(60)).await;
        assert_eq!(lane.lane_id(), "lane-1");
    }

    /// Submit MUST delegate to the controller: a prompt op is
    /// accepted (no error) and the broadcast receiver handed out
    /// by `subscribe` is live.
    #[tokio::test]
    async fn submit_delegates_to_controller() {
        let lane = lane(Duration::from_secs(60)).await;
        let mut rx = lane.subscribe();
        lane.submit(SessionOp::Prompt {
            content: "hi".to_string(),
            priority: 1,
        })
        .await
        .unwrap();
        // Drain: the run starts and the first broadcast arrives
        // (or the timeout fires — either proves the submit was
        // delegated, since a rejected submit would have errored
        // above).
        let _ =
            tokio::time::timeout(Duration::from_millis(200), rx.recv()).await;
    }

    /// `SessionLane` MUST be object-safe so future fan-out can
    /// hold `Arc<dyn SessionLane>`.
    #[tokio::test]
    async fn lane_is_object_safe() {
        let lane = lane(Duration::from_secs(60)).await;
        let dyn_lane: Arc<dyn SessionLane> = Arc::new(lane);
        assert_eq!(dyn_lane.lane_id(), "lane-1");
        let _rx = dyn_lane.subscribe();
        dyn_lane
            .submit(SessionOp::Prompt {
                content: "via-dyn".to_string(),
                priority: 1,
            })
            .await
            .unwrap();
    }

    /// `close` MUST delegate to the controller shutdown: after
    /// close, a further submit is rejected.
    #[tokio::test]
    async fn close_then_submit_rejected() {
        let lane = lane(Duration::from_secs(60)).await;
        let controller = Arc::clone(lane.controller());
        lane.close().await.unwrap();
        // The Shutdown op is processed asynchronously by the
        // controller loop; wait for the loop to actually exit
        // before asserting the next submit fails.
        tokio::time::timeout(Duration::from_millis(500), async {
            while controller.is_alive() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        let err = lane
            .submit(SessionOp::Prompt {
                content: "late".to_string(),
                priority: 1,
            })
            .await
            .unwrap_err();
        assert!(matches!(err, LaneError::Submit(_)));
    }

    /// Compile-time: the controller accessor round-trips.
    #[tokio::test]
    async fn controller_accessor_returns_inner() {
        let lane = lane(Duration::from_secs(60)).await;
        assert_eq!(lane.controller().session_id(), "lane-1");
    }
}
