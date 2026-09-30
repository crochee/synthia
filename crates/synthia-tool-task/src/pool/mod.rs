//! Subagent admission pooling + invocation records — pi-subagents
//! `agent-manager` parity.
//!
//! There are **two independent concurrency lanes, never one**
//! (pi-subagents §agent-manager):
//!
//! - [`Slot::Background`] is bounded by
//!   [`DEFAULT_BACKGROUND_CONCURRENCY`] (overridable via
//!   [`SubagentPool::with_background_limit`]) and **queues** the
//!   excess in FIFO order. Detached work — a child nobody is
//!   blocked on — belongs here.
//! - [`Slot::Foreground`] is unbounded by default. A blocking child
//!   already holds its parent's turn, so charging it to a
//!   session-wide pool would let a saturated lane starve work the
//!   parent could have done itself. Bound it with
//!   [`SubagentPool::with_foreground_limit`] when parallel children
//!   thrash a shared model.
//! - [`Slot::Uncharged`] is the exemption every **nested** child
//!   takes: its parent is blocked awaiting it, so gating it behind a
//!   slot the parent already holds would deadlock the pair
//!   (pi-subagents `occupiesPoolSlot` / `occupiesForegroundSlot`).
//!
//! # A pure state machine
//!
//! The pool owns no threads, no timers, and no clock. It decides
//! *whether* a spawn may proceed now, keeps the queue order, and
//! records what happened; the caller admits, spawns, and releases.
//! That keeps the whole concurrency policy unit-testable without a
//! runtime, and lets an embedder drive it from whatever executor it
//! already has.
//!
//! # Lifecycle
//!
//! ```text
//! admit(slot) ──► Admission::Run(ticket) ──► spawn ──► release(ticket)
//!              └► Admission::Queued{…}   ──► wait for a release
//! ```
//!
//! A queued child is admitted by the [`SubagentPool::release`] that
//! frees a slot in its own lane: the returned [`QueuedToken`] carries
//! the ticket the caller releases once that child settles, and its
//! [`QueuedToken::seq`] matches the `seq` the child was queued with,
//! so a driver that buffers pending spawns can find the one being
//! admitted. A blocking caller that cannot park itself on the queue
//! (the `task` tool seam) uses [`SubagentPool::try_admit`] instead:
//! it never enqueues work that nobody would start.
//!
//! # Invocation records
//!
//! [`InvocationRecord`] is the observability half: after a child
//! settles the caller records it with
//! [`SubagentPool::record_completion`], and the last
//! [`MAX_TOMBSTONES`] finished children stay addressable by id
//! through [`SubagentPool::lookup`]. The requested-vs-effective
//! model / tier pair is the point (pi-subagents `onResolved`): a
//! substitution is visible after the fact instead of silent.
//!
//! ## Module layout
//!
//! - `slot` — [`Slot`] + [`DEFAULT_BACKGROUND_CONCURRENCY`]:
//!   the lanes a spawn is charged to.
//! - `admission` — [`AdmissionTicket`] / [`Admission`] /
//!   [`QueuedToken`]: the admission lifecycle types.
//! - `record` — [`InvocationStatus`] / [`InvocationRecord`] /
//!   [`MAX_TOMBSTONES`]: the observability half.
//! - `state` — [`SubagentPool`] / [`PoolCounts`] /
//!   [`SharedSubagentPool`]: the state machine itself.

mod admission;
mod record;
mod slot;
mod state;

pub use admission::{Admission, AdmissionTicket, QueuedToken};
pub use record::{InvocationRecord, InvocationStatus, MAX_TOMBSTONES};
pub use slot::{DEFAULT_BACKGROUND_CONCURRENCY, Slot};
pub use state::{PoolCounts, SharedSubagentPool, SubagentPool};

#[cfg(test)]
mod tests {
    use synthia_harness::SessionEndReason;
    use synthia_provider::{ModelTier, TokenUsage};

    use super::*;

    fn running(admission: Admission) -> AdmissionTicket {
        match admission {
            Admission::Run(ticket) => ticket,
            other => panic!("expected an admission, got {other:?}"),
        }
    }

    fn queued(admission: Admission) -> (u64, usize) {
        match admission {
            Admission::Queued { seq, position } => (seq, position),
            other => panic!("expected a queue position, got {other:?}"),
        }
    }

    fn record(depth: usize, status: InvocationStatus) -> InvocationRecord {
        InvocationRecord {
            depth,
            status,
            ..Default::default()
        }
    }

    #[test]
    fn background_lane_is_bounded_and_queues_in_fifo_order() {
        let mut pool = SubagentPool::new().with_background_limit(2);
        let first = running(pool.admit(Slot::Background));
        let second = running(pool.admit(Slot::Background));
        assert_eq!(pool.active(Slot::Background), 2);

        let (third_seq, third_position) = queued(pool.admit(Slot::Background));
        assert_eq!(third_position, 0, "the 3rd child waits at the head");
        let (fourth_seq, fourth_position) =
            queued(pool.admit(Slot::Background));
        assert_eq!(fourth_position, 1, "FIFO: the 4th waits behind the 3rd");
        assert_eq!(pool.queued(Slot::Background), 2);
        assert_eq!(pool.next_queued(), Some(Slot::Background));

        // Releasing a running child hands its slot to the *earliest*
        // waiter, never the last one.
        let token = pool.release(first).expect("a waiter takes the slot");
        assert_eq!(token.seq(), third_seq);
        assert_eq!(token.slot(), Slot::Background);
        assert_eq!(pool.active(Slot::Background), 2);
        assert_eq!(pool.queued(Slot::Background), 1);

        let token = pool.release(second).expect("the last waiter");
        assert_eq!(token.seq(), fourth_seq);
        assert_eq!(pool.queued_len(), 0);
        assert_eq!(pool.next_queued(), None);
        assert_eq!(pool.active(Slot::Background), 2);

        assert!(pool.release(token.ticket()).is_none());
        assert_eq!(pool.active(Slot::Background), 1);
        assert!(!pool.is_idle());
    }

    #[test]
    fn foreground_lane_is_unbounded_by_default() {
        let mut pool = SubagentPool::new();
        assert_eq!(pool.foreground_limit(), None);

        let mut tickets = Vec::new();
        for _ in 0..64 {
            tickets.push(running(pool.admit(Slot::Foreground)));
        }
        assert_eq!(pool.active(Slot::Foreground), 64);
        assert_eq!(pool.queued_len(), 0);

        for ticket in tickets {
            assert!(pool.release(ticket).is_none());
        }
        assert!(pool.is_idle());
    }

    #[test]
    fn bounded_foreground_lane_queues_and_dequeues() {
        let mut pool = SubagentPool::new().with_foreground_limit(1);
        assert_eq!(pool.foreground_limit(), Some(1));

        let first = running(pool.admit(Slot::Foreground));
        let (seq, position) = queued(pool.admit(Slot::Foreground));
        assert_eq!(position, 0);
        assert_eq!(pool.queued(Slot::Foreground), 1);

        let token = pool.release(first).expect("the waiter takes the slot");
        assert_eq!(token.seq(), seq);
        assert_eq!(pool.active(Slot::Foreground), 1);
        assert_eq!(pool.queued_len(), 0);
    }

    #[test]
    fn nested_children_hold_no_slot() {
        // The deadlock guard: a nested child runs while the only
        // background slot is held by its own parent.
        let mut pool = SubagentPool::new().with_background_limit(1);
        let parent = running(pool.admit(Slot::Background));
        let nested = running(pool.admit(Slot::Uncharged));

        assert_eq!(pool.active(Slot::Uncharged), 1);
        assert_eq!(pool.active(Slot::Background), 1);
        assert_eq!(pool.queued_len(), 0);

        // The bound still holds for a second *top-level* child.
        let (_, position) = queued(pool.admit(Slot::Background));
        assert_eq!(position, 0);

        // Releasing the nested child frees nothing: it held no slot.
        assert!(pool.release(nested).is_none());
        assert_eq!(pool.queued_len(), 1);
        let token = pool.release(parent).expect("the freed slot drains");
        assert_eq!(token.slot(), Slot::Background);
    }

    #[test]
    fn release_drains_only_the_freed_lane() {
        let mut pool = SubagentPool::new()
            .with_background_limit(1)
            .with_foreground_limit(1);
        let background = running(pool.admit(Slot::Background));
        let foreground = running(pool.admit(Slot::Foreground));
        let (background_waiting, _) = queued(pool.admit(Slot::Background));
        let (foreground_waiting, _) = queued(pool.admit(Slot::Foreground));
        assert_eq!(pool.next_queued(), Some(Slot::Background));

        let token = pool.release(foreground).expect("foreground waiter");
        assert_eq!(token.seq(), foreground_waiting);
        assert_eq!(pool.queued(Slot::Foreground), 0);
        assert_eq!(
            pool.queued(Slot::Background),
            1,
            "a saturated background queue must not be drained by a \
             foreground release"
        );
        assert_eq!(pool.next_queued(), Some(Slot::Background));

        let token = pool.release(background).expect("background waiter");
        assert_eq!(token.seq(), background_waiting);
        // Both lanes now hold the child each release admitted: the
        // queue is empty, and the pool is busy with them.
        assert_eq!(pool.queued_len(), 0);
        assert_eq!(pool.counts().active(), 2);
        assert!(!pool.is_idle());
    }

    #[test]
    fn try_admit_never_queues() {
        let mut pool = SubagentPool::new().with_background_limit(1);
        let held = pool.try_admit(Slot::Background).expect("room");
        assert!(pool.try_admit(Slot::Background).is_none());
        assert_eq!(pool.queued_len(), 0, "a refusal must not enqueue");
        assert!(
            pool.try_admit(Slot::Uncharged).is_some(),
            "nested children are never gated"
        );
        pool.release(held);
        assert!(pool.try_admit(Slot::Background).is_some());
    }

    #[test]
    fn stale_and_foreign_tickets_free_nothing() {
        let mut pool = SubagentPool::new().with_background_limit(1);
        let held = running(pool.admit(Slot::Background));
        let _waiting = queued(pool.admit(Slot::Background));
        assert!(pool.release(held).is_some());

        assert!(
            pool.release(held).is_none(),
            "a double release frees nothing"
        );
        assert_eq!(pool.active(Slot::Background), 1);
        assert_eq!(pool.queued_len(), 0);

        let mut other = SubagentPool::new();
        let foreign = running(other.admit(Slot::Background));
        assert!(
            pool.release(foreign).is_none(),
            "another pool's ticket frees nothing"
        );
        assert_eq!(pool.active(Slot::Background), 1);
    }

    #[test]
    fn tombstones_keep_the_last_hundred_children_addressable() {
        let mut pool = SubagentPool::new();
        for index in 0..MAX_TOMBSTONES + 1 {
            pool.record_completion(
                format!("child-{index}"),
                record(1, InvocationStatus::Completed),
            );
        }
        assert_eq!(pool.tombstone_count(), MAX_TOMBSTONES);
        assert!(pool.lookup("child-0").is_none(), "oldest entry evicted");
        assert!(pool.lookup("child-1").is_some());
        assert_eq!(
            pool.tombstones().next().map(|(id, _)| id),
            Some("child-100"),
            "tombstones list newest first"
        );
        assert_eq!(pool.tombstones().count(), MAX_TOMBSTONES);
        assert_eq!(pool.counts().tombstones, MAX_TOMBSTONES);
    }

    #[test]
    fn lookup_returns_the_latest_record_for_a_repeated_id() {
        let mut pool = SubagentPool::new();
        pool.record_completion("child-1", record(1, InvocationStatus::Failed));
        pool.record_completion(
            "child-1",
            record(2, InvocationStatus::Completed),
        );
        let found = pool.lookup("child-1").expect("addressable");
        assert_eq!(found.depth, 2);
        assert_eq!(found.status, InvocationStatus::Completed);
    }

    #[test]
    fn invocation_record_captures_requested_versus_effective() {
        let mut found = InvocationRecord {
            requested_model: Some("claude-opus".to_string()),
            effective_model: Some("claude-sonnet".to_string()),
            requested_tier: Some(ModelTier::Large),
            effective_tier: Some(ModelTier::Medium),
            depth: 2,
            status: InvocationStatus::Completed,
            ..Default::default()
        };
        assert!(found.model_substituted());
        assert!(found.tier_substituted());

        found.add_usage(&TokenUsage {
            prompt_tokens: 10,
            completion_tokens: 5,
            total_tokens: 15,
            cache_read_tokens: Some(3),
            ..Default::default()
        });
        found.add_usage(&TokenUsage {
            prompt_tokens: 1,
            completion_tokens: 2,
            total_tokens: 3,
            reasoning_tokens: Some(2),
            ..Default::default()
        });
        assert_eq!(found.usage.prompt_tokens, 11);
        assert_eq!(found.usage.completion_tokens, 7);
        assert_eq!(found.usage.total_tokens, 18);
        assert_eq!(found.usage.cache_read_tokens, Some(3));
        assert_eq!(found.usage.reasoning_tokens, Some(2));
        assert_eq!(found.usage.cache_write_tokens, None);

        let honoured = InvocationRecord {
            requested_model: Some("same".to_string()),
            effective_model: Some("same".to_string()),
            requested_tier: Some(ModelTier::Small),
            effective_tier: Some(ModelTier::Small),
            ..Default::default()
        };
        assert!(!honoured.model_substituted());
        assert!(!honoured.tier_substituted());
        assert!(
            !InvocationRecord::default().model_substituted(),
            "an unknown effective model makes no substitution claim"
        );
    }

    #[test]
    fn invocation_status_classifies_every_end_reason() {
        let cases = [
            (SessionEndReason::Completed, InvocationStatus::Completed),
            (SessionEndReason::Cancelled, InvocationStatus::Cancelled),
            (
                SessionEndReason::Error("boom".to_string()),
                InvocationStatus::Failed,
            ),
            (
                SessionEndReason::MaxIterations,
                InvocationStatus::MaxIterations,
            ),
        ];
        for (reason, expected) in cases {
            assert_eq!(InvocationStatus::from_end_reason(&reason), expected);
        }
        assert!(InvocationStatus::Completed.is_success());
        assert!(!InvocationStatus::MaxIterations.is_success());
        assert_eq!(InvocationStatus::Failed.as_str(), "failed");
        assert_eq!(InvocationStatus::MaxIterations.as_str(), "max_iterations");
    }

    #[test]
    fn counts_snapshot_covers_every_lane() {
        let mut pool = SubagentPool::new().with_background_limit(1);
        let _running = running(pool.admit(Slot::Background));
        let _nested = running(pool.admit(Slot::Uncharged));
        let _waiting = queued(pool.admit(Slot::Background));
        pool.record_completion(
            "child-1",
            record(1, InvocationStatus::Completed),
        );

        let counts = pool.counts();
        assert_eq!(counts.background_active, 1);
        assert_eq!(counts.uncharged_active, 1);
        assert_eq!(counts.foreground_active, 0);
        assert_eq!(counts.background_queued, 1);
        assert_eq!(counts.foreground_queued, 0);
        assert_eq!(counts.tombstones, 1);
        assert_eq!(counts.active(), 2);
        assert_eq!(counts.queued(), 1);
        assert!(!pool.is_idle());
    }

    #[test]
    fn limits_are_normalized_and_depths_map_to_lanes() {
        assert_eq!(
            SubagentPool::new().background_limit(),
            DEFAULT_BACKGROUND_CONCURRENCY
        );
        assert_eq!(
            SubagentPool::new()
                .with_background_limit(0)
                .background_limit(),
            1
        );
        assert_eq!(
            SubagentPool::new()
                .with_foreground_limit(0)
                .foreground_limit(),
            None
        );
        assert_eq!(
            SubagentPool::new()
                .with_foreground_limit(4)
                .foreground_limit(),
            Some(4)
        );

        assert_eq!(Slot::for_depth(0), Slot::Foreground);
        assert_eq!(Slot::for_depth(1), Slot::Uncharged);
        assert_eq!(Slot::for_depth(MAX_TOMBSTONES), Slot::Uncharged);
        assert_eq!(Slot::Background.as_str(), "background");
        assert_eq!(Slot::Foreground.as_str(), "foreground");
        assert_eq!(Slot::Uncharged.as_str(), "uncharged");
    }
}
