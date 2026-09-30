//! Sub-agent session router: every delegated child runs in its own
//! first-class session instead of inlining its trace into the parent.
//!
//! The `task` tool forwards a child's events into the parent run's
//! stream wrapped in `AgentEvent::Agent(AgentMeta, inner)`. Before
//! this module existed the controller persisted those wrapped events
//! into the **parent's** log and broadcast them on the parent's SSE —
//! the parent transcript became a flat dump of every child's tool
//! calls, and the child had no addressable session of its own.
//!
//! The router inverts the ownership: a wrapped event arriving at
//! [`ControllerInner::persist_and_broadcast`](super::controller) is
//! handed here and never reaches the parent's log or broadcast.
//! Instead, per child (keyed by `AgentMeta::child_session_id`, a ULID
//! the task tool mints):
//!
//! 1. a `subagent_enter` header row + the dispatch prompt (carried on
//!    the child's first event, see `AgentMeta::prompt`) open the
//!    child's own session log — created through the same
//!    [`SessionRegistry`] every top-level session uses, so the child
//!    shows up in `GET /api/v1/sessions` and replays through
//!    `GET /api/v1/sessions/{id}` exactly like a user conversation;
//! 2. every durable inner event is appended **unwrapped** to the
//!    child's log, in the same row shape a top-level run writes
//!    (`{"type":"Model","data":…}`), so the transcript folds with the
//!    standard machinery;
//! 3. a child `SessionEnded` closes the child log with a
//!    `subagent_exit` footer;
//! 4. the **parent's** log gets only the two lightweight
//!    `subagent_enter` / `subagent_exit` marker rows — enough for the
//!    parent transcript to link the delegation, without any of the
//!    child's content;
//! 5. if a controller for the child session exists (a client opened
//!    its SSE stream), the inner event is broadcast through it, so
//!    the child session streams live like any other.
//!
//! Nested delegation (a child delegating a grandchild) arrives as
//! `Agent(m1, Agent(m2, inner))`. The recursion keeps each level's
//! trace in its own session: the `Agent(m2, …)` wrapper row lands in
//! child `m1`'s log (its record of having delegated), `inner` lands
//! in child `m2`'s log, and only the depth-1 markers reach the root
//! parent's `RunLog`.

use std::sync::Arc;

use dashmap::DashSet;
use synthia::{
    core::Clock,
    harness::{AgentEvent, AgentMeta, SystemEvent},
    session::{manager::SessionRegistry, subagent_enter, subagent_exit},
};

use super::controller::RunLog;
use crate::state::ActiveSessions;

/// Where a level's marker rows go. Only the ROOT parent's ledgered
/// writer receives them: deeper levels' delegations are already
/// recorded in their parent's log as the wrapped `Agent(meta, …)`
/// row, and a second marker row there would say the same thing
/// twice.
enum ParentSink<'a> {
    Root(&'a mut RunLog),
    Deeper,
}

/// Routes delegated sub-agent events into per-child sessions.
pub struct SubagentRouter {
    registry: Arc<SessionRegistry>,
    active: Arc<ActiveSessions>,
    clock: synthia::core::SharedClock,
    /// Child session ids whose `subagent_enter` header has been
    /// written. Ids are ULIDs minted per delegation, so an id is
    /// never legitimately reused; the set only guards the
    /// double-write race between two events of the same child.
    entered: DashSet<String>,
}

impl SubagentRouter {
    /// Build a router over the process-wide session registry and
    /// controller cache.
    #[must_use]
    pub fn new(
        registry: Arc<SessionRegistry>,
        active: Arc<ActiveSessions>,
        clock: synthia::core::SharedClock,
    ) -> Self {
        Self {
            registry,
            active,
            clock,
            entered: DashSet::new(),
        }
    }

    /// Route one wrapped child event (and, recursively, any deeper
    /// wrapped levels) into the child sessions it belongs to.
    ///
    /// `root_log` is the ROOT parent's ledgered writer: only the
    /// depth-1 markers go through it, because it is the one log a
    /// compaction checkpoint validates provenance against.
    pub(crate) async fn route(
        &self,
        user_id: &str,
        parent_session_id: &str,
        meta: &AgentMeta,
        event: &AgentEvent,
        root_log: &mut RunLog,
    ) {
        if let Err(error) = self
            .route_level(
                user_id,
                parent_session_id,
                meta,
                event,
                ParentSink::Root(root_log),
            )
            .await
        {
            tracing::warn!(
                target: "synthia.session",
                parent_session_id,
                child_session_id = %meta.child_session_id,
                %error,
                "subagent router: dropping child event after write failure"
            );
        }
    }

    /// One level of the recursion. See the module docs for the row
    /// layout; errors bubble as `anyhow` because every failure mode
    /// is a sink write the caller can only log.
    async fn route_level(
        &self,
        user_id: &str,
        parent_session_id: &str,
        meta: &AgentMeta,
        event: &AgentEvent,
        parent: ParentSink<'_>,
    ) -> anyhow::Result<()> {
        // Async recursion must be boxed; the depth is bounded by
        // `MAX_SUBAGENT_DEPTH`, so one box per level is bounded too.
        Box::pin(self.route_level_inner(
            user_id,
            parent_session_id,
            meta,
            event,
            parent,
        ))
        .await
    }

    async fn route_level_inner(
        &self,
        user_id: &str,
        parent_session_id: &str,
        meta: &AgentMeta,
        event: &AgentEvent,
        mut parent: ParentSink<'_>,
    ) -> anyhow::Result<()> {
        let child_id = meta.child_session_id.trim().to_string();
        if !is_safe_session_id(&child_id) {
            return Ok(());
        }
        let depth: u32 = meta.parent_depth.try_into().unwrap_or(u32::MAX);
        let agent_name = meta.agent_name.as_deref().unwrap_or("unknown");
        let sink = self.registry.sink(user_id, &child_id);

        if self.entered.insert(child_id.clone()) {
            let header = serde_json::to_value(subagent_enter(
                &child_id,
                parent_session_id,
                depth,
                agent_name,
            ))?;
            sink.append(&self.stamp_ts(header.clone())).await?;
            if let Some(prompt) = meta.prompt.as_ref().filter(|p| !p.is_empty())
            {
                sink.append(&serde_json::json!({
                    "type": "UserInput",
                    "data": { "text": prompt },
                }))
                .await?;
            }
            self.append_to_parent(&mut parent, header).await?;
        }

        match event {
            AgentEvent::Agent(inner_meta, inner_event) => {
                // The wrapped level below is THIS child's record of
                // having delegated; the innermost events belong to
                // the grandchild's own session.
                if event.is_durable() {
                    let value = serde_json::to_value(event)?;
                    sink.append(&self.stamp_ts(value)).await?;
                }
                self.route_level(
                    user_id,
                    &child_id,
                    inner_meta,
                    inner_event,
                    ParentSink::Deeper,
                )
                .await?;
            }
            inner => {
                if inner.is_durable() {
                    let value = serde_json::to_value(inner)?;
                    sink.append(&self.stamp_ts(value)).await?;
                }
                if let AgentEvent::System(SystemEvent::SessionEnded {
                    reason,
                }) = inner
                {
                    let footer = serde_json::to_value(subagent_exit(
                        &child_id,
                        depth,
                        reason.as_str(),
                    ))?;
                    sink.append(&self.stamp_ts(footer.clone())).await?;
                    self.append_to_parent(&mut parent, footer).await?;
                }
            }
        }

        // Live tail: a client that opened the child session's stream
        // (which materialised a controller through the normal
        // get-or-create path) sees the event on that session's own
        // broadcast, exactly like a top-level run's subscribers do.
        if let Some(controller) =
            self.active.get(&(user_id.to_string(), child_id.clone()))
        {
            controller.broadcast_event(event);
        }
        Ok(())
    }

    /// Write a marker row to the root parent's ledgered log. Deeper
    /// levels pass [`ParentSink::Deeper`] and write nothing — see
    /// the enum's docs.
    async fn append_to_parent(
        &self,
        parent: &mut ParentSink<'_>,
        row: serde_json::Value,
    ) -> anyhow::Result<()> {
        if let ParentSink::Root(log) = parent {
            log.append_typed(row).await?;
        }
        Ok(())
    }

    /// Stamp an RFC 3339 timestamp on a row the typed builders leave
    /// blank. Rows written straight to a sink (everything except the
    /// root parent's `RunLog` path, which stamps its own) get their
    /// wall clock here so replay views can order them.
    fn stamp_ts(&self, mut value: serde_json::Value) -> serde_json::Value {
        if let Some(obj) = value.as_object_mut() {
            obj.insert(
                "ts".to_string(),
                serde_json::Value::String(self.clock.now().to_rfc3339()),
            );
        }
        value
    }

    /// Session registry the router creates child sinks through.
    #[must_use]
    pub fn registry(&self) -> &Arc<SessionRegistry> {
        &self.registry
    }
}

/// The same shape [`SessionRegistry`] validates ids with, checked
/// here defensively: the child id reaches `sessions_root`'s path via
/// `sink()`, which does not validate on its own. ULIDs always pass;
/// this guards a future delegator that mints something else.
fn is_safe_session_id(id: &str) -> bool {
    !id.is_empty()
        && !id.contains('/')
        && !id.contains('\\')
        && !id.contains('\0')
        && !id.starts_with('.')
}

#[cfg(test)]
mod tests {
    use synthia::{
        core::SharedClock,
        provider::{ContentPart, TextContent},
        session::SurfaceLedger,
    };

    use super::*;

    fn text_event(text: &str) -> AgentEvent {
        AgentEvent::Model(ContentPart::Text(TextContent {
            text: text.to_string(),
            cache_control: None,
        }))
    }

    fn ended_event() -> AgentEvent {
        AgentEvent::System(SystemEvent::SessionEnded {
            reason: synthia::harness::SessionEndReason::Completed,
        })
    }

    fn row_types(events: &[serde_json::Value]) -> Vec<String> {
        events
            .iter()
            .map(|ev| {
                ev.get("type")
                    .and_then(|t| t.as_str())
                    .unwrap_or("?")
                    .to_string()
            })
            .collect()
    }

    /// The contract the whole design hangs on: a delegated child's
    /// durable events land in the child's own session log, while the
    /// parent's log keeps only the enter / exit markers — the child's
    /// trace never inlines into the parent transcript.
    #[tokio::test]
    async fn child_events_land_in_their_own_session() {
        let dir = tempfile::tempdir().expect("tempdir");
        let registry = Arc::new(SessionRegistry::new(dir.path().to_path_buf()));
        let router = SubagentRouter::new(
            Arc::clone(&registry),
            Arc::new(ActiveSessions::new()),
            SharedClock::system(),
        );
        let parent_sink = registry.sink("u1", "parent-1");
        let mut log = RunLog::new(
            Arc::clone(&parent_sink),
            Arc::new(SurfaceLedger::new()),
            0,
            SharedClock::system(),
        );

        let meta = AgentMeta::new("parent-1", "child-1", 1)
            .with_agent_name(Some("scout".to_string()));
        // First forwarded event carries the dispatch prompt.
        let first_meta = meta.clone().with_prompt(Some("map the repo".into()));
        router
            .route(
                "u1",
                "parent-1",
                &first_meta,
                &text_event("step one"),
                &mut log,
            )
            .await;
        router
            .route("u1", "parent-1", &meta, &ended_event(), &mut log)
            .await;
        let child_events = registry
            .sink("u1", "child-1")
            .read()
            .await
            .expect("child read");
        assert_eq!(
            row_types(&child_events),
            vec!["subagent_enter", "UserInput", "Model", "subagent_exit"],
            "child log: header, prompt, own events, footer"
        );
        assert_eq!(
            child_events[1]
                .pointer("/data/text")
                .and_then(|t| t.as_str()),
            Some("map the repo")
        );
        assert_eq!(
            child_events[0]
                .pointer("/data/agent_name")
                .and_then(|a| a.as_str()),
            Some("scout")
        );
        assert_eq!(
            child_events[0]
                .pointer("/data/parent_session_id")
                .and_then(|p| p.as_str()),
            Some("parent-1")
        );

        let parent_events = parent_sink.read().await.expect("parent read");
        assert_eq!(
            row_types(&parent_events),
            vec!["subagent_enter", "subagent_exit"],
            "parent log: markers only, none of the child's content"
        );
    }

    /// A grandchild delegation arrives as `Agent(m1, Agent(m2, …))`;
    /// each level's trace stays in its own session and the root
    /// parent still sees only its direct child's markers.
    #[tokio::test]
    async fn nested_delegation_splits_per_level() {
        let dir = tempfile::tempdir().expect("tempdir");
        let registry = Arc::new(SessionRegistry::new(dir.path().to_path_buf()));
        let router = SubagentRouter::new(
            Arc::clone(&registry),
            Arc::new(ActiveSessions::new()),
            SharedClock::system(),
        );
        let parent_sink = registry.sink("u1", "root");
        let mut log = RunLog::new(
            Arc::clone(&parent_sink),
            Arc::new(SurfaceLedger::new()),
            0,
            SharedClock::system(),
        );

        let m1 = AgentMeta::new("root", "child-1", 1)
            .with_agent_name(Some("scout".to_string()));
        let m2 = AgentMeta::new("child-1", "child-2", 2)
            .with_agent_name(Some("deep".to_string()));
        let nested = AgentEvent::Agent(m2, Box::new(text_event("deep work")));
        router
            .route(
                "u1",
                "root",
                &m1.clone().with_prompt(Some("outer".into())),
                &nested,
                &mut log,
            )
            .await;

        let grandchild = registry
            .sink("u1", "child-2")
            .read()
            .await
            .expect("grandchild read");
        assert_eq!(
            row_types(&grandchild),
            vec!["subagent_enter", "Model"],
            "grandchild log: its own header + its own event"
        );
        assert_eq!(
            grandchild[0]
                .pointer("/data/parent_session_id")
                .and_then(|p| p.as_str()),
            Some("child-1"),
            "grandchild attribution names the child, not the root"
        );

        let child = registry
            .sink("u1", "child-1")
            .read()
            .await
            .expect("child read");
        assert_eq!(
            row_types(&child),
            vec!["subagent_enter", "UserInput", "Agent"],
            "child log: header, prompt, and the wrapped grandchild trace"
        );
        let parent_events = parent_sink.read().await.expect("parent read");
        assert_eq!(
            row_types(&parent_events),
            vec!["subagent_enter"],
            "root parent: only its direct child's enter marker"
        );
    }
}
