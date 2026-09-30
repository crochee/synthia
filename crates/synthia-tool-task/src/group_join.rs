//! Batched completion notification for background subagents
//! (pi-subagents `group-join.ts` parity).
//!
//! When an orchestrator fans out several background subagents,
//! notifying the main agent once per completion produces a burst
//! of interruptions. [`GroupJoin`] holds those completions and
//! emits **one** consolidated delivery per group when the group
//! finishes — or, if it takes too long, a partial delivery with
//! whatever finished, so a single slow straggler cannot stall the
//! notification forever.
//!
//! It lives beside the delegation plugin it serves rather than in the
//! harness: a host that never runs subagents has nothing to batch. A
//! pure value type, it composes with the [`pool`](crate::pool) — the pool
//! decides *whether* a child starts, this decides *when the host hears
//! about it*.
//!
//! ## Semantics (per group)
//!
//! | Event | Result |
//! |---|---|
//! | `on_complete` for an agent in no group | [`GroupOutcome::Pass`] — notify individually |
//! | `on_complete`, other members still pending | [`GroupOutcome::Held`] — buffered, nothing emitted |
//! | `on_complete`, last outstanding member | [`GroupOutcome::Deliver`] with `partial: false`, group removed |
//! | `on_tick` after `initial_timeout` | [`GroupOutcome`] partial delivery of the finished subset; the group stays alive for stragglers on a shorter `straggler_timeout` |
//! | `on_tick` in straggler mode | partial delivery of **only** the newly finished members |
//!
//! A given agent id is delivered **at most once**, so a partial
//! delivery followed by the remaining members never repeats a
//! result. Once every member has been delivered, the group is
//! removed and further ticks are silent.
//!
//! ## Runtime neutrality
//!
//! This is a pure state machine: it owns no clock and no timer.
//! The caller passes `now` (`chrono::DateTime<Utc>`) and arms its
//! own timer from [`GroupJoin::next_deadline`], so it works on any
//! executor and tests are deterministic.
//!
//! ```
//! use chrono::{DateTime, TimeDelta, Utc};
//! use synthia_tool_task::{GroupJoin, GroupOutcome};
//!
//! let mut join = GroupJoin::new();
//! // Register BEFORE launching the agents — a completion for an
//! // unknown id can only be reported individually.
//! join.register_group("panel", ["a".to_string(), "b".to_string()]);
//!
//! let t0 = DateTime::<Utc>::from_timestamp(0, 0).unwrap();
//! assert!(matches!(join.on_complete("a", t0), GroupOutcome::Held));
//! assert!(join.next_deadline() == Some(t0 + TimeDelta::seconds(30)));
//!
//! let delivery = match join.on_complete("b", t0) {
//!     GroupOutcome::Deliver(delivery) => delivery,
//!     other => panic!("expected a delivery, got {other:?}"),
//! };
//! assert!(!delivery.partial);
//! assert_eq!(delivery.agent_ids, ["a", "b"]);
//! ```

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, TimeDelta, Utc};

/// Default window for a group to complete before a partial
/// delivery (pi's `DEFAULT_TIMEOUT`).
pub const DEFAULT_GROUP_TIMEOUT: TimeDelta = TimeDelta::seconds(30);

/// Re-batch window for members that finish after a partial
/// delivery (pi's `STRAGGLER_TIMEOUT`).
pub const DEFAULT_STRAGGLER_TIMEOUT: TimeDelta = TimeDelta::seconds(15);

/// What the caller should do after reporting a completion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroupOutcome {
    /// The agent belongs to no group — notify individually.
    Pass,
    /// Buffered: the group is still waiting on other members.
    Held,
    /// Emit one consolidated notification for the group.
    Deliver(Delivery),
}

/// One consolidated notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delivery {
    /// The group this delivery belongs to.
    pub group_id: String,
    /// Members whose results are in this delivery, in completion
    /// order. Never repeats an id across deliveries.
    pub agent_ids: Vec<String>,
    /// `true` when the group had not finished and the window
    /// expired (a straggler is still running).
    pub partial: bool,
}

#[derive(Debug)]
struct Group {
    /// Members still outstanding.
    pending: HashSet<String>,
    /// Members finished but not yet delivered, in completion
    /// order.
    finished: Vec<String>,
    /// Everything already handed to the caller.
    delivered: HashSet<String>,
    /// When the current window expires. `None` until the first
    /// member finishes (an idle group has no deadline).
    deadline: Option<DateTime<Utc>>,
    /// True once a partial delivery happened — later windows use
    /// the shorter straggler timeout.
    straggler: bool,
}

/// Coordinates consolidated completion notifications for grouped
/// background agents. See the module docs for the state machine.
#[derive(Debug)]
pub struct GroupJoin {
    groups: HashMap<String, Group>,
    agent_to_group: HashMap<String, String>,
    group_timeout: TimeDelta,
    straggler_timeout: TimeDelta,
}

impl Default for GroupJoin {
    fn default() -> Self {
        Self::new()
    }
}

impl GroupJoin {
    /// With pi's default windows (30 s / 15 s).
    #[must_use]
    pub fn new() -> Self {
        Self::with_timeouts(DEFAULT_GROUP_TIMEOUT, DEFAULT_STRAGGLER_TIMEOUT)
    }

    /// With explicit windows.
    #[must_use]
    pub fn with_timeouts(
        group_timeout: TimeDelta,
        straggler_timeout: TimeDelta,
    ) -> Self {
        Self {
            groups: HashMap::new(),
            agent_to_group: HashMap::new(),
            group_timeout,
            straggler_timeout,
        }
    }

    /// Declare that `agent_ids` form a group whose completions
    /// should be delivered together.
    ///
    /// Register **before** launching the agents: a completion for
    /// an id that is not yet grouped can only be reported
    /// individually ([`GroupOutcome::Pass`]), because this type
    /// keeps no ledger of past completions.
    ///
    /// Re-registering an existing group id replaces it. An empty
    /// `agent_ids` registers nothing.
    pub fn register_group(
        &mut self,
        group_id: impl Into<String>,
        agent_ids: impl IntoIterator<Item = String>,
    ) {
        let group_id = group_id.into();
        let members: Vec<String> = agent_ids.into_iter().collect();
        if members.is_empty() {
            return;
        }
        // Drop any previous registration under this id so a
        // re-registration cannot leave dangling agent mappings.
        self.forget(&group_id);
        for id in &members {
            self.agent_to_group.insert(id.clone(), group_id.clone());
        }
        self.groups.insert(
            group_id,
            Group {
                pending: members.into_iter().collect(),
                finished: Vec::new(),
                delivered: HashSet::new(),
                deadline: None,
                straggler: false,
            },
        );
    }

    /// Report that `agent_id` finished at `now`.
    pub fn on_complete(
        &mut self,
        agent_id: &str,
        now: DateTime<Utc>,
    ) -> GroupOutcome {
        let Some(group_id) = self.agent_to_group.get(agent_id).cloned() else {
            return GroupOutcome::Pass;
        };
        let (group_timeout, straggler_timeout) =
            (self.group_timeout, self.straggler_timeout);
        let Some(group) = self.groups.get_mut(&group_id) else {
            return GroupOutcome::Pass;
        };
        if group.delivered.contains(agent_id) {
            // Already handed over in an earlier partial delivery.
            return GroupOutcome::Held;
        }
        group.pending.remove(agent_id);
        group.finished.push(agent_id.to_string());
        if group.deadline.is_none() {
            let window = if group.straggler {
                straggler_timeout
            } else {
                group_timeout
            };
            group.deadline = Some(now + window);
        }
        if group.pending.is_empty() {
            return GroupOutcome::Deliver(self.take_delivery(&group_id, false));
        }
        GroupOutcome::Held
    }

    /// Deliver every group whose window expired at `now`.
    ///
    /// Call this from the caller's timer; `now` is supplied by the
    /// caller so this type stays clock-free.
    pub fn on_tick(&mut self, now: DateTime<Utc>) -> Vec<Delivery> {
        let straggler_timeout = self.straggler_timeout;
        let expired: Vec<String> = self
            .groups
            .iter()
            .filter(|(_, group)| group.deadline.is_some_and(|d| d <= now))
            .map(|(id, _)| id.clone())
            .collect();
        let mut deliveries = Vec::new();
        for group_id in expired {
            let Some(group) = self.groups.get(&group_id) else {
                continue;
            };
            if group.finished.is_empty() {
                // Nothing finished yet inside the window: re-arm
                // silently rather than delivering an empty batch.
                let next = now + straggler_timeout;
                if let Some(group) = self.groups.get_mut(&group_id) {
                    group.deadline = Some(next);
                }
                continue;
            }
            deliveries.push(self.take_delivery(&group_id, true));
            // If stragglers remain, re-arm with the shorter
            // window measured from *now* — leaving the expired
            // deadline in place would fire again immediately.
            if let Some(group) = self.groups.get_mut(&group_id) {
                group.straggler = true;
                group.deadline = Some(now + straggler_timeout);
            }
        }
        deliveries
    }

    /// Earliest window expiry across all groups, so the caller can
    /// arm a timer. `None` when nothing is pending.
    #[must_use]
    pub fn next_deadline(&self) -> Option<DateTime<Utc>> {
        self.groups.values().filter_map(|g| g.deadline).min()
    }

    /// Is `agent_id` a member of a live group?
    #[must_use]
    pub fn is_grouped(&self, agent_id: &str) -> bool {
        self.agent_to_group.contains_key(agent_id)
    }

    /// Number of live groups.
    #[must_use]
    pub fn len(&self) -> usize {
        self.groups.len()
    }

    /// No live groups.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    /// Drop a group (and its member mappings). Later completions
    /// for its members are reported individually.
    pub fn forget(&mut self, group_id: &str) {
        if let Some(group) = self.groups.remove(group_id) {
            // Every member must lose its mapping — including
            // already-delivered ones, or a retired group would
            // leak entries and keep reporting `is_grouped` true.
            for id in group
                .pending
                .iter()
                .chain(&group.finished)
                .chain(&group.delivered)
            {
                self.agent_to_group.remove(id);
            }
        }
    }

    /// Remove finished members from the group and build the
    /// delivery, deleting the group once every member has been
    /// delivered.
    fn take_delivery(&mut self, group_id: &str, partial: bool) -> Delivery {
        let Some(group) = self.groups.get_mut(group_id) else {
            return Delivery {
                group_id: group_id.to_string(),
                agent_ids: Vec::new(),
                partial,
            };
        };
        let agent_ids = std::mem::take(&mut group.finished);
        for id in &agent_ids {
            group.delivered.insert(id.clone());
        }
        let done = group.pending.is_empty();
        if done {
            group.deadline = None;
        }
        let delivery = Delivery {
            group_id: group_id.to_string(),
            agent_ids,
            partial,
        };
        if done {
            self.forget(group_id);
        }
        delivery
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeDelta;

    use super::*;

    fn t0() -> DateTime<Utc> {
        DateTime::<Utc>::from_timestamp(1_000, 0).expect("valid timestamp")
    }

    fn group(join: &mut GroupJoin, ids: &[&str]) {
        join.register_group(
            "g",
            ids.iter().map(|s| (*s).to_string()).collect::<Vec<_>>(),
        );
    }

    fn delivered(outcome: GroupOutcome) -> Delivery {
        match outcome {
            GroupOutcome::Deliver(d) => d,
            other => panic!("expected a delivery, got {other:?}"),
        }
    }

    #[test]
    fn ungrouped_completion_passes_through() {
        let mut join = GroupJoin::new();
        assert_eq!(join.on_complete("solo", t0()), GroupOutcome::Pass);
        assert!(!join.is_grouped("solo"));
        assert!(join.is_empty());
    }

    #[test]
    fn group_delivers_once_when_the_last_member_finishes() {
        let mut join = GroupJoin::new();
        group(&mut join, &["a", "b", "c"]);

        assert_eq!(join.on_complete("a", t0()), GroupOutcome::Held);
        assert_eq!(join.on_complete("c", t0()), GroupOutcome::Held);
        let delivery = delivered(join.on_complete("b", t0()));

        assert_eq!(delivery.group_id, "g");
        assert!(!delivery.partial);
        // Completion order, so the notification reads
        // chronologically.
        assert_eq!(delivery.agent_ids, ["a", "c", "b"]);
        // Group retired: nothing pending, nothing grouped, no
        // deadline to arm.
        assert!(join.is_empty());
        assert!(!join.is_grouped("a"));
        assert_eq!(join.next_deadline(), None);
    }

    #[test]
    fn a_slow_member_does_not_stall_the_notification() {
        let mut join = GroupJoin::with_timeouts(
            TimeDelta::seconds(30),
            TimeDelta::seconds(15),
        );
        group(&mut join, &["fast", "slow"]);

        assert_eq!(join.on_complete("fast", t0()), GroupOutcome::Held);
        // The window is armed from the first completion.
        assert_eq!(join.next_deadline(), Some(t0() + TimeDelta::seconds(30)));

        // Nothing yet: a tick before the deadline is silent.
        assert!(join.on_tick(t0() + TimeDelta::seconds(29)).is_empty());

        let deliveries = join.on_tick(t0() + TimeDelta::seconds(30));
        assert_eq!(deliveries.len(), 1);
        assert_eq!(deliveries[0].agent_ids, ["fast"]);
        assert!(deliveries[0].partial);
        // The straggler is still outstanding, so the group lives
        // on — now on the shorter window.
        assert!(join.is_grouped("slow"));
        assert_eq!(join.next_deadline(), Some(t0() + TimeDelta::seconds(45)));
    }

    #[test]
    fn stragglers_are_delivered_without_repeating_results() {
        let mut join = GroupJoin::new();
        group(&mut join, &["a", "b"]);

        join.on_complete("a", t0());
        let first = join.on_tick(t0() + DEFAULT_GROUP_TIMEOUT).remove(0);
        assert_eq!(first.agent_ids, ["a"]);
        assert!(first.partial);

        // The straggler finishes inside its window and is
        // delivered alone — 'a' is not repeated.
        let second =
            delivered(join.on_complete("b", t0() + DEFAULT_GROUP_TIMEOUT));
        assert_eq!(second.agent_ids, ["b"]);
        assert!(!second.partial);
        assert!(join.is_empty());
    }

    #[test]
    fn a_reported_completion_is_never_delivered_twice() {
        let mut join = GroupJoin::new();
        group(&mut join, &["a", "b"]);

        join.on_complete("a", t0());
        join.on_tick(t0() + DEFAULT_GROUP_TIMEOUT);
        // A duplicate report for the delivered member is held,
        // not emitted again.
        assert_eq!(
            join.on_complete("a", t0() + DEFAULT_GROUP_TIMEOUT),
            GroupOutcome::Held
        );
        let second =
            delivered(join.on_complete("b", t0() + DEFAULT_GROUP_TIMEOUT));
        assert_eq!(second.agent_ids, ["b"]);
    }

    #[test]
    fn an_empty_window_is_re_armed_instead_of_delivering_nothing() {
        let mut join = GroupJoin::new();
        group(&mut join, &["a"]);

        // No member has finished, so there is nothing to arm or
        // deliver yet.
        assert_eq!(join.next_deadline(), None);
        assert!(join.on_tick(t0() + TimeDelta::hours(1)).is_empty());
        assert!(join.is_grouped("a"));
    }

    #[test]
    fn forget_returns_members_to_individual_notification() {
        let mut join = GroupJoin::new();
        group(&mut join, &["a", "b"]);
        assert!(join.is_grouped("a"));

        join.forget("g");
        assert!(!join.is_grouped("a"));
        assert!(join.is_empty());
        assert_eq!(join.on_complete("a", t0()), GroupOutcome::Pass);
    }

    #[test]
    fn registering_an_empty_set_is_a_no_op() {
        let mut join = GroupJoin::new();
        join.register_group("g", Vec::new());
        assert!(join.is_empty());
        assert_eq!(join.next_deadline(), None);
    }

    #[test]
    fn re_registering_a_group_id_replaces_it() {
        let mut join = GroupJoin::new();
        group(&mut join, &["a", "b"]);
        join.register_group("g", ["c".to_string(), "d".to_string()]);

        // Old members are no longer grouped…
        assert_eq!(join.on_complete("a", t0()), GroupOutcome::Pass);
        // …and the new set still joins as one.
        assert_eq!(join.on_complete("c", t0()), GroupOutcome::Held);
        let delivery = delivered(join.on_complete("d", t0()));
        assert_eq!(delivery.agent_ids, ["c", "d"]);
    }

    #[test]
    fn two_groups_are_joined_independently() {
        let mut join = GroupJoin::new();
        join.register_group("left", ["a".to_string()]);
        join.register_group("right", ["b".to_string()]);

        let left = delivered(join.on_complete("a", t0()));
        assert_eq!(left.group_id, "left");
        let right = delivered(join.on_complete("b", t0()));
        assert_eq!(right.group_id, "right");
        assert!(join.is_empty());
    }
}
